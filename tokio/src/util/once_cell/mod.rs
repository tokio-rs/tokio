#[cfg(feature = "parking_lot")]
#[path = "imp_pl.rs"]
mod imp;

#[cfg(not(feature = "parking_lot"))]
#[path = "imp_std.rs"]
mod imp;

/// Thread-safe, blocking version of `OnceCell`.
pub(crate) mod sync {
    use std::{
        cell::Cell,
        fmt,
        ops::{Deref, DerefMut},
        panic::RefUnwindSafe,
    };

    use super::imp::OnceCell as Imp;

    /// A thread-safe cell which can be written to only once.
    ///
    /// `OnceCell` provides `&` references to the contents without RAII guards.
    ///
    /// Reading a non-`None` value out of `OnceCell` establishes a
    /// happens-before relationship with a corresponding write. For example, if
    /// thread A initializes the cell with `get_or_init(f)`, and thread B
    /// subsequently reads the result of this call, B also observes all the side
    /// effects of `f`.
    pub(crate) struct OnceCell<T>(Imp<T>);

    impl<T> Default for OnceCell<T> {
        fn default() -> OnceCell<T> {
            OnceCell::new()
        }
    }

    impl<T: fmt::Debug> fmt::Debug for OnceCell<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self.get() {
                Some(v) => f.debug_tuple("OnceCell").field(v).finish(),
                None => f.write_str("OnceCell(Uninit)"),
            }
        }
    }

    impl<T: Clone> Clone for OnceCell<T> {
        fn clone(&self) -> OnceCell<T> {
            match self.get() {
                Some(value) => Self::with_value(value.clone()),
                None => Self::new(),
            }
        }

        fn clone_from(&mut self, source: &Self) {
            match (self.get_mut(), source.get()) {
                (Some(this), Some(source)) => this.clone_from(source),
                _ => *self = source.clone(),
            }
        }
    }

    impl<T> From<T> for OnceCell<T> {
        fn from(value: T) -> Self {
            Self::with_value(value)
        }
    }

    impl<T: PartialEq> PartialEq for OnceCell<T> {
        fn eq(&self, other: &OnceCell<T>) -> bool {
            self.get() == other.get()
        }
    }

    impl<T: Eq> Eq for OnceCell<T> {}

    impl<T> OnceCell<T> {
        /// Creates a new empty cell.
        pub(crate) const fn new() -> OnceCell<T> {
            OnceCell(Imp::new())
        }

        /// Creates a new initialized cell.
        pub(crate) const fn with_value(value: T) -> OnceCell<T> {
            OnceCell(Imp::with_value(value))
        }

        /// Gets the reference to the underlying value.
        ///
        /// Returns `None` if the cell is empty, or being initialized. This
        /// method never blocks.
        pub(crate) fn get(&self) -> Option<&T> {
            if self.0.is_initialized() {
                // Safe b/c value is initialized.
                Some(unsafe { self.get_unchecked() })
            } else {
                None
            }
        }

        /// Gets the mutable reference to the underlying value.
        ///
        /// Returns `None` if the cell is empty.
        ///
        /// This method is allowed to violate the invariant of writing to a `OnceCell`
        /// at most once because it requires `&mut` access to `self`. As with all
        /// interior mutability, `&mut` access permits arbitrary modification:
        pub(crate) fn get_mut(&mut self) -> Option<&mut T> {
            self.0.get_mut()
        }

        /// Get the reference to the underlying value, without checking if the
        /// cell is initialized.
        ///
        /// # Safety
        ///
        /// Caller must ensure that the cell is in initialized state, and that
        /// the contents are acquired by (synchronized to) this thread.
        pub(crate) unsafe fn get_unchecked(&self) -> &T {
            self.0.get_unchecked()
        }

        /// Gets the contents of the cell, initializing it with `f` if the cell
        /// was empty.
        ///
        /// Many threads may call `get_or_init` concurrently with different
        /// initializing functions, but it is guaranteed that only one function
        /// will be executed.
        ///
        /// # Panics
        ///
        /// If `f` panics, the panic is propagated to the caller, and the cell
        /// remains uninitialized.
        ///
        /// It is an error to reentrantly initialize the cell from `f`. The
        /// exact outcome is unspecified. Current implementation deadlocks, but
        /// this may be changed to a panic in the future.
        pub(crate) fn get_or_init<F>(&self, f: F) -> &T
        where
            F: FnOnce() -> T,
        {
            enum Void {}
            match self.get_or_try_init(|| Ok::<T, Void>(f())) {
                Ok(val) => val,
                Err(void) => match void {},
            }
        }

        /// Gets the contents of the cell, initializing it with `f` if
        /// the cell was empty. If the cell was empty and `f` failed, an
        /// error is returned.
        ///
        /// # Panics
        ///
        /// If `f` panics, the panic is propagated to the caller, and
        /// the cell remains uninitialized.
        ///
        /// It is an error to reentrantly initialize the cell from `f`.
        /// The exact outcome is unspecified. Current implementation
        /// deadlocks, but this may be changed to a panic in the future.
        pub(crate) fn get_or_try_init<F, E>(&self, f: F) -> Result<&T, E>
        where
            F: FnOnce() -> Result<T, E>,
        {
            // Fast path check
            if let Some(value) = self.get() {
                return Ok(value);
            }
            self.0.initialize(f)?;

            // Safe b/c value is initialized.
            debug_assert!(self.0.is_initialized());
            Ok(unsafe { self.get_unchecked() })
        }
    }

    /// A value which is initialized on the first access.
    ///
    /// This type is thread-safe and can be used in statics.
    pub(crate) struct Lazy<T, F = fn() -> T> {
        cell: OnceCell<T>,
        init: Cell<Option<F>>,
    }

    impl<T: fmt::Debug, F> fmt::Debug for Lazy<T, F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Lazy")
                .field("cell", &self.cell)
                .field("init", &"..")
                .finish()
        }
    }

    // We never create a `&F` from a `&Lazy<T, F>` so it is fine to not impl
    // `Sync` for `F`. We do create a `&mut Option<F>` in `force`, but this is
    // properly synchronized, so it only happens once so it also does not
    // contribute to this impl.
    unsafe impl<T, F: Send> Sync for Lazy<T, F> where OnceCell<T>: Sync {}
    // auto-derived `Send` impl is OK.

    impl<T, F: RefUnwindSafe> RefUnwindSafe for Lazy<T, F> where OnceCell<T>: RefUnwindSafe {}

    impl<T, F> Lazy<T, F> {
        /// Creates a new lazy value with the given initializing
        /// function.
        pub(crate) const fn new(f: F) -> Lazy<T, F> {
            Lazy {
                cell: OnceCell::new(),
                init: Cell::new(Some(f)),
            }
        }
    }

    impl<T, F: FnOnce() -> T> Lazy<T, F> {
        /// Forces the evaluation of this lazy value and
        /// returns a reference to the result. This is equivalent
        /// to the `Deref` impl, but is explicit.
        pub(crate) fn force(this: &Lazy<T, F>) -> &T {
            this.cell.get_or_init(|| match this.init.take() {
                Some(f) => f(),
                None => panic!("Lazy instance has previously been poisoned"),
            })
        }
    }

    impl<T, F: FnOnce() -> T> Deref for Lazy<T, F> {
        type Target = T;
        fn deref(&self) -> &T {
            Lazy::force(self)
        }
    }

    impl<T, F: FnOnce() -> T> DerefMut for Lazy<T, F> {
        fn deref_mut(&mut self) -> &mut T {
            Lazy::force(self);
            self.cell.get_mut().unwrap_or_else(|| unreachable!())
        }
    }

    impl<T: Default> Default for Lazy<T> {
        /// Creates a new lazy value using `Default` as the initializing function.
        fn default() -> Lazy<T> {
            Lazy::new(T::default)
        }
    }

    /// ```compile_fail
    /// struct S(*mut ());
    /// unsafe impl Sync for S {}
    ///
    /// fn share<T: Sync>(_: &T) {}
    /// share(&once_cell::sync::OnceCell::<S>::new());
    /// ```
    ///
    /// ```compile_fail
    /// struct S(*mut ());
    /// unsafe impl Sync for S {}
    ///
    /// fn share<T: Sync>(_: &T) {}
    /// share(&once_cell::sync::Lazy::<S>::new(|| unimplemented!()));
    /// ```
    fn _dummy() {}
}

unsafe fn take_unchecked<T>(val: &mut Option<T>) -> T {
    match val.take() {
        Some(it) => it,
        None => {
            debug_assert!(false);
            std::hint::unreachable_unchecked()
        }
    }
}
