//! Abstraction over blocking and unblocking the current thread.
//!
//! Provides an abstraction over blocking the current thread. This is similar to
//! the park / unpark constructs provided by [`std`] but made generic. This
//! allows embedding custom functionality to perform when the thread is blocked.
//!
//! A blocked [`Park`][p] instance is unblocked by calling [`unpark`] on its
//! [`Unpark`][up] handle.
//!
//! The [`ParkThread`] struct implements [`Park`][p] using
//! [`thread::park`][`std`] to put the thread to sleep. The Tokio reactor also
//! implements park, but uses [`mio::Poll`][mio] to block the thread instead.
//!
//! The [`Park`][p] trait is composable. A timer implementation might decorate a
//! [`Park`][p] implementation by checking if any timeouts have elapsed after
//! the inner [`Park`][p] implementation unblocks.
//!
//! # Model
//!
//! Conceptually, each [`Park`][p] instance has an associated token, which is
//! initially not present:
//!
//! * The [`park`] method blocks the current thread unless or until the token
//!   is available, at which point it atomically consumes the token.
//! * The [`unpark`] method atomically makes the token available if it wasn't
//!   already.
//!
//! Some things to note:
//!
//! * If [`unpark`] is called before [`park`], the next call to [`park`] will
//! **not** block the thread.
//! * **Spurious** wakeups are permitted, i.e., the [`park`] method may unblock
//!   even if [`unpark`] was not called.
//! * [`park_timeout`] does the same as [`park`] but allows specifying a maximum
//!   time to block the thread for.
//!
//! [`std`]: https://doc.rust-lang.org/std/thread/fn.park.html
//! [`thread::park`]: https://doc.rust-lang.org/std/thread/fn.park.html
//! [`ParkThread`]: struct.ParkThread.html
//! [p]: trait.Park.html
//! [`park`]: trait.Park.html#tymethod.park
//! [`park_timeout`]: trait.Park.html#tymethod.park_timeout
//! [`unpark`]: trait.Unpark.html#tymethod.unpark
//! [up]: trait.Unpark.html
//! [mio]: https://docs.rs/mio/0.6/mio/struct.Poll.html

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_utils::sync::{Parker, Unparker};

/// Block the current thread.
///
/// See [module documentation][mod] for more details.
///
/// [mod]: ../index.html
pub trait Park {
    /// Unpark handle type for the `Park` implementation.
    type Unpark: Unpark;

    /// Error returned by `park`
    type Error;

    /// Get a new `Unpark` handle associated with this `Park` instance.
    fn unpark(&self) -> Self::Unpark;

    /// Block the current thread unless or until the token is available.
    ///
    /// A call to `park` does not guarantee that the thread will remain blocked
    /// forever, and callers should be prepared for this possibility. This
    /// function may wakeup spuriously for any reason.
    ///
    /// See [module documentation][mod] for more details.
    ///
    /// # Panics
    ///
    /// This function **should** not panic, but ultimately, panics are left as
    /// an implementation detail. Refer to the documentation for the specific
    /// `Park` implementation
    ///
    /// [mod]: ../index.html
    fn park(&mut self) -> Result<(), Self::Error>;

    /// Park the current thread for at most `duration`.
    ///
    /// This function is the same as `park` but allows specifying a maximum time
    /// to block the thread for.
    ///
    /// Same as `park`, there is no guarantee that the thread will remain
    /// blocked for any amount of time. Spurious wakeups are permitted for any
    /// reason.
    ///
    /// See [module documentation][mod] for more details.
    ///
    /// # Panics
    ///
    /// This function **should** not panic, but ultimately, panics are left as
    /// an implementation detail. Refer to the documentation for the specific
    /// `Park` implementation
    ///
    /// [mod]: ../index.html
    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error>;
}

/// Unblock a thread blocked by the associated [`Park`] instance.
///
/// See [module documentation][mod] for more details.
///
/// [mod]: ../index.html
/// [`Park`]: trait.Park.html
pub trait Unpark: Sync + Send + 'static {
    /// Unblock a thread that is blocked by the associated `Park` handle.
    ///
    /// Calling `unpark` atomically makes available the unpark token, if it is
    /// not already available.
    ///
    /// See [module documentation][mod] for more details.
    ///
    /// # Panics
    ///
    /// This function **should** not panic, but ultimately, panics are left as
    /// an implementation detail. Refer to the documentation for the specific
    /// `Unpark` implementation
    ///
    /// [mod]: ../index.html
    fn unpark(&self);
}

impl Unpark for Box<dyn Unpark> {
    fn unpark(&self) {
        (**self).unpark()
    }
}

impl Unpark for Arc<dyn Unpark> {
    fn unpark(&self) {
        (**self).unpark()
    }
}

/// Blocks the current thread using a condition variable.
///
/// Implements the [`Park`] functionality by using a condition variable. An
/// atomic variable is also used to avoid using the condition variable if
/// possible.
///
/// The condition variable is cached in a thread-local variable and is shared
/// across all `ParkThread` instances created on the same thread. This also
/// means that an instance of `ParkThread` might be unblocked by a handle
/// associated with a different `ParkThread` instance.
#[derive(Debug)]
pub struct ParkThread {
    _anchor: PhantomData<Rc<()>>,
}

/// Error returned by [`ParkThread`]
///
/// This currently is never returned, but might at some point in the future.
///
/// [`ParkThread`]: struct.ParkThread.html
#[derive(Debug)]
pub struct ParkError {
    _p: (),
}

/// Unblocks a thread that was blocked by `ParkThread`.
#[derive(Clone, Debug)]
pub struct UnparkThread {
    inner: Unparker,
}

thread_local! {
    static CURRENT_PARKER: Parker = Parker::new();
}

// ===== impl ParkThread =====

impl ParkThread {
    /// Create a new `ParkThread` handle for the current thread.
    ///
    /// This type cannot be moved to other threads, so it should be created on
    /// the thread that the caller intends to park.
    pub fn new() -> ParkThread {
        ParkThread {
            _anchor: PhantomData,
        }
    }

    /// Get a reference to the `ParkThread` handle for this thread.
    fn with_current<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&Parker) -> R,
    {
        CURRENT_PARKER.with(|inner| f(inner))
    }
}

impl Park for ParkThread {
    type Unpark = UnparkThread;
    type Error = ParkError;

    fn unpark(&self) -> Self::Unpark {
        let inner = self.with_current(|inner| inner.unparker().clone());
        UnparkThread { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.with_current(|inner| inner.park());
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.with_current(|inner| inner.park_timeout(duration));
        Ok(())
    }
}

// ===== impl UnparkThread =====

impl Unpark for UnparkThread {
    fn unpark(&self) {
        self.inner.unpark();
    }
}
