use crate::loom::cell::UnsafeCell;
use crate::loom::sync::atomic::AtomicU8;
use crate::loom::sync::Mutex;
use crate::util::linked_list::LinkedList;
use crate::util::{linked_list, WakeList};
use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::marker::PhantomPinned;
use std::mem::MaybeUninit;
use std::ops::Drop;
use std::pin::Pin;
use std::ptr;
use std::ptr::NonNull;
use std::sync::atomic::Ordering;
use std::task::{ready, Context, Poll, Waker};

// This file contains an implementation of an OnceCell. Its synchronization relies
// on an atomic state with 3 possible values:
// - STATE_UNSET: the cell is uninitialized
// - STATE_SET: the cell is initialized, its value can be accessed
// - STATE_LOCKED: the cell is locked, its value can be set
//
// Initializing the cell is done in 3 steps:
// - acquire the cell lock by setting its state to `STATE_LOCKED` with a CAS
// - writing the cell value
// - setting the state to `STATE_SET`

/// Cell is uninitialized.
const STATE_UNSET: u8 = 0;
/// Cell is initialized.
const STATE_SET: u8 = 1;
/// Cell is locked for initialization.
const STATE_LOCKED: u8 = 2;

/// A thread-safe cell that can be written to only once.
///
/// A `OnceCell` is typically used for global variables that need to be
/// initialized once on first use, but need no further changes. The `OnceCell`
/// in Tokio allows the initialization procedure to be asynchronous.
///
/// # Examples
///
/// ```
/// use tokio::sync::OnceCell;
///
/// async fn some_computation() -> u32 {
///     1 + 1
/// }
///
/// static ONCE: OnceCell<u32> = OnceCell::const_new();
///
/// #[tokio::main]
/// async fn main() {
///     let result = ONCE.get_or_init(some_computation).await;
///     assert_eq!(*result, 2);
/// }
/// ```
///
/// It is often useful to write a wrapper method for accessing the value.
///
/// ```
/// use tokio::sync::OnceCell;
///
/// static ONCE: OnceCell<u32> = OnceCell::const_new();
///
/// async fn get_global_integer() -> &'static u32 {
///     ONCE.get_or_init(|| async {
///         1 + 1
///     }).await
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let result = get_global_integer().await;
///     assert_eq!(*result, 2);
/// }
/// ```
pub struct OnceCell<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
    waiters: Mutex<LinkedList<Waiter, <Waiter as linked_list::Link>::Target>>,
    /// The current number of available permits in the semaphore.
    #[cfg(all(tokio_unstable, feature = "tracing"))]
    resource_span: tracing::Span,
}

impl<T> Default for OnceCell<T> {
    fn default() -> OnceCell<T> {
        OnceCell::new()
    }
}

impl<T: fmt::Debug> fmt::Debug for OnceCell<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("OnceCell")
            .field("value", &self.get())
            .finish()
    }
}

impl<T: Clone> Clone for OnceCell<T> {
    fn clone(&self) -> OnceCell<T> {
        OnceCell::new_with(self.get().cloned())
    }
}

impl<T: PartialEq> PartialEq for OnceCell<T> {
    fn eq(&self, other: &OnceCell<T>) -> bool {
        self.get() == other.get()
    }
}

impl<T: Eq> Eq for OnceCell<T> {}

impl<T> Drop for OnceCell<T> {
    fn drop(&mut self) {
        if self.initialized_mut() {
            unsafe {
                self.value
                    .with_mut(|ptr| ptr::drop_in_place((*ptr).as_mut_ptr()));
            };
        }
    }
}

impl<T> From<T> for OnceCell<T> {
    #[track_caller]
    fn from(value: T) -> Self {
        Self::new_with(Some(value))
    }
}

impl<T> OnceCell<T> {
    /// Creates a new empty `OnceCell` instance.
    #[track_caller]
    pub fn new() -> Self {
        Self::new_with(None)
    }

    /// Creates a new empty `OnceCell` instance.
    ///
    /// Equivalent to `OnceCell::new`, except that it can be used in static
    /// variables.
    ///
    /// When using the `tracing` [unstable feature], a `OnceCell` created with
    /// `const_new` will not be instrumented. As such, it will not be visible
    /// in [`tokio-console`]. Instead, [`OnceCell::new`] should be used to
    /// create an instrumented object if that is needed.
    ///
    /// # Example
    ///
    /// ```
    /// use tokio::sync::OnceCell;
    ///
    /// static ONCE: OnceCell<u32> = OnceCell::const_new();
    ///
    /// async fn get_global_integer() -> &'static u32 {
    ///     ONCE.get_or_init(|| async {
    ///         1 + 1
    ///     }).await
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let result = get_global_integer().await;
    ///     assert_eq!(*result, 2);
    /// }
    /// ```
    ///
    /// [`tokio-console`]: https://github.com/tokio-rs/console
    /// [unstable feature]: crate#unstable-features
    #[cfg(not(all(loom, test)))]
    pub const fn const_new() -> Self {
        OnceCell {
            state: AtomicU8::new(STATE_UNSET),
            value: UnsafeCell::new(MaybeUninit::uninit()),
            waiters: Mutex::const_new(LinkedList::new()),
            #[cfg(all(tokio_unstable, feature = "tracing"))]
            resource_span: tracing::Span::none(),
        }
    }

    /// Creates a new `OnceCell` that contains the provided value, if any.
    ///
    /// If the `Option` is `None`, this is equivalent to `OnceCell::new`.
    ///
    /// [`OnceCell::new`]: crate::sync::OnceCell::new
    // Once https://github.com/rust-lang/rust/issues/73255 lands
    // and tokio MSRV is bumped to the rustc version with it stabilised,
    // we can make this function available in const context,
    // by creating `Semaphore::const_new_closed`.
    #[track_caller]
    pub fn new_with(value: Option<T>) -> Self {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let resource_span = {
            let location = std::panic::Location::caller();

            tracing::trace_span!(
                parent: None,
                "runtime.resource",
                concrete_type = "OnceCell",
                kind = "Sync",
                loc.file = location.file(),
                loc.line = location.line(),
                loc.col = location.column(),
            )
        };
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        resource_span.in_scope(|| {
            tracing::trace!(
                target: "runtime::resource::state_update",
                state = "unset",
            );
        });
        let (state, value) = match value {
            Some(v) => (STATE_SET, MaybeUninit::new(v)),
            None => (STATE_UNSET, MaybeUninit::uninit()),
        };
        Self {
            state: AtomicU8::new(state),
            value: UnsafeCell::new(value),
            waiters: Mutex::new(LinkedList::new()),
            #[cfg(all(tokio_unstable, feature = "tracing"))]
            resource_span,
        }
    }

    /// Creates a new `OnceCell` that contains the provided value.
    ///
    /// # Example
    ///
    /// When using the `tracing` [unstable feature], a `OnceCell` created with
    /// `const_new_with` will not be instrumented. As such, it will not be
    /// visible in [`tokio-console`]. Instead, [`OnceCell::new_with`] should be
    /// used to create an instrumented object if that is needed.
    ///
    /// ```
    /// use tokio::sync::OnceCell;
    ///
    /// static ONCE: OnceCell<u32> = OnceCell::const_new_with(1);
    ///
    /// async fn get_global_integer() -> &'static u32 {
    ///     ONCE.get_or_init(|| async {
    ///         1 + 1
    ///     }).await
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let result = get_global_integer().await;
    ///     assert_eq!(*result, 1);
    /// }
    /// ```
    ///
    /// [`tokio-console`]: https://github.com/tokio-rs/console
    /// [unstable feature]: crate#unstable-features
    #[cfg(not(all(loom, test)))]
    pub const fn const_new_with(value: T) -> Self {
        OnceCell {
            state: AtomicU8::new(STATE_SET),
            value: UnsafeCell::new(MaybeUninit::new(value)),
            waiters: Mutex::const_new(LinkedList::new()),
            #[cfg(all(tokio_unstable, feature = "tracing"))]
            resource_span: tracing::Span::none(),
        }
    }

    /// Returns `true` if the `OnceCell` currently contains a value, and `false`
    /// otherwise.
    pub fn initialized(&self) -> bool {
        // Using acquire ordering so any threads that read a true from this
        // atomic is able to read the value.
        self.state.load(Ordering::Acquire) == STATE_SET
    }

    /// Returns `true` if the `OnceCell` currently contains a value, and `false`
    /// otherwise.
    fn initialized_mut(&mut self) -> bool {
        self.state.with_mut(|s| *s == STATE_SET)
    }

    // SAFETY: The OnceCell must not be empty.
    unsafe fn get_unchecked(&self) -> &T {
        &*self.value.with(|ptr| (*ptr).as_ptr())
    }

    // SAFETY: The OnceCell must not be empty.
    unsafe fn get_unchecked_mut(&mut self) -> &mut T {
        &mut *self.value.with_mut(|ptr| (*ptr).as_mut_ptr())
    }

    /// # Safety
    ///
    /// `set_value` must be called after having locked the state
    unsafe fn set_value(&self, value: T) -> &T {
        // SAFETY: the state is locked
        unsafe {
            self.value.with_mut(|ptr| (*ptr).as_mut_ptr().write(value));
        }

        // Using release ordering so any threads that read a true from this
        // atomic is able to read the value we just stored.
        self.state.store(STATE_SET, Ordering::Release);
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        self.resource_span.in_scope(|| {
            tracing::trace!(
                target: "runtime::resource::state_update",
                state = "set",
            );
        });
        self.notify_initialized();

        // SAFETY: We just initialized the cell.
        unsafe { self.get_unchecked() }
    }

    /// Returns a reference to the value currently stored in the `OnceCell`, or
    /// `None` if the `OnceCell` is empty.
    pub fn get(&self) -> Option<&T> {
        if self.initialized() {
            Some(unsafe { self.get_unchecked() })
        } else {
            None
        }
    }

    /// Returns a mutable reference to the value currently stored in the
    /// `OnceCell`, or `None` if the `OnceCell` is empty.
    ///
    /// Since this call borrows the `OnceCell` mutably, it is safe to mutate the
    /// value inside the `OnceCell` — the mutable borrow statically guarantees
    /// no other references exist.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        if self.initialized_mut() {
            Some(unsafe { self.get_unchecked_mut() })
        } else {
            None
        }
    }

    /// Sets the value of the `OnceCell` to the given value if the `OnceCell` is
    /// empty.
    ///
    /// If the `OnceCell` already has a value, this call will fail with an
    /// [`SetError::AlreadyInitializedError`].
    ///
    /// If the `OnceCell` is empty, but some other task is currently trying to
    /// set the value, this call will fail with [`SetError::InitializingError`].
    ///
    /// [`SetError::AlreadyInitializedError`]: crate::sync::SetError::AlreadyInitializedError
    /// [`SetError::InitializingError`]: crate::sync::SetError::InitializingError
    pub fn set(&self, value: T) -> Result<(), SetError<T>> {
        // special handling for ZSTs
        if std::mem::size_of::<T>() == 0 {
            return self.set_zst(value);
        }
        // Try to lock the state, using acquire `Acquire` ordering for both success
        // and failure to have happens-before relation with previous initialization
        // attempts.
        match self.state.compare_exchange(
            STATE_UNSET,
            STATE_LOCKED,
            Ordering::Acquire,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                #[cfg(all(tokio_unstable, feature = "tracing"))]
                self.resource_span.in_scope(|| {
                    tracing::trace!(
                        target: "runtime::resource::state_update",
                        state = "locked",
                    );
                });
                // SAFETY: state has been locked
                unsafe { self.set_value(value) };
                Ok(())
            }
            Err(STATE_LOCKED) => Err(SetError::InitializingError(value)),
            Err(STATE_SET) => Err(SetError::AlreadyInitializedError(value)),
            // SAFETY: all possible values of state are covered
            Err(_) => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    /// ZSTs don't need a two-phase writing, as they don't even need to be written;
    /// `MaybeUninit::assume_init` is indeed always safe and valid for ZSTs.
    fn set_zst(&self, value: T) -> Result<(), SetError<T>> {
        assert_eq!(std::mem::size_of::<T>(), 0);
        // Even if there is no value set, user may expect to have `set`
        // happens-before `wait_initialized`, and successful `set` happens-before
        // failing `set`, so release ordering is used for store and acquire for load.
        match self.state.compare_exchange(
            STATE_UNSET,
            STATE_SET,
            Ordering::Release,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                #[cfg(all(tokio_unstable, feature = "tracing"))]
                self.resource_span.in_scope(|| {
                    tracing::trace!(
                        target: "runtime::resource::state_update",
                        state = "set",
                    );
                });
                // Forget the value, as `T` could implement `Drop`.
                std::mem::forget(value);
                self.notify_initialized();
                Ok(())
            }
            Err(STATE_LOCKED) => Err(SetError::InitializingError(value)),
            Err(STATE_SET) => Err(SetError::AlreadyInitializedError(value)),
            // SAFETY: all possible values of state are covered
            Err(_) => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    /// Notify all waiting tasks.
    fn notify_initialized(&self) {
        let mut wakers = WakeList::new();
        let mut waiters = self.waiters.lock();
        loop {
            while wakers.can_push() {
                if let Some(mut waiter) = waiters.pop_front() {
                    // Safety: we hold the lock, so we can access the waker.
                    if let Some(waker) = unsafe { waiter.as_mut().waker.take() } {
                        wakers.push(waker);
                    }
                } else {
                    // Release the lock before notifying
                    drop(waiters);
                    wakers.wake_all();
                    return;
                }
            }
            drop(waiters);
            wakers.wake_all();
            waiters = self.waiters.lock();
        }
    }

    /// Notify one initializer task.
    fn notify_unlocked(&self) {
        let mut waiters = self.waiters.lock();
        if let Some(mut waiter) = waiters.drain_filter(|w| w.is_initializer).next() {
            // SAFETY: we hold the lock, so we can access the waker.
            let waker = unsafe { waiter.as_mut().waker.take() };
            drop(waiters);
            waker.unwrap().wake();
        }
    }

    fn poll_wait(&self, cx: &mut Context<'_>, node: Pin<&mut Waiter>, queued: bool) -> Poll<u8> {
        // Checks if there is no need to wait, using acquire ordering
        // as value may be accessed after in case of success.
        let state = self.state.load(Ordering::Acquire);
        if state == STATE_SET || node.is_initializer && state == STATE_UNSET {
            return Poll::Ready(state);
        }
        // Clone the waker before locking, a waker clone can be triggering arbitrary code.
        let waker = cx.waker().clone();
        let mut waiters = self.waiters.lock();
        // Checks the state, this time with the lock held, so the state access
        // is synchronized by the lock. If the waiter has been notified, then the
        // state is ensured to have been modified.
        let state = self.state.load(Ordering::Acquire);
        if state == STATE_SET || node.is_initializer && state == STATE_UNSET {
            return Poll::Ready(state);
        }
        // SAFETY: node is not moved out of pinned reference
        unsafe {
            let node = node.get_unchecked_mut();
            // waker is only accessed with the waiters lock acquired
            node.waker = Some(waker);
            // If the waiter is not already in the wait queue, enqueue it.
            if !queued {
                waiters.push_front(NonNull::from(node));
            }
        }
        Poll::Pending
    }

    /// Waits for the `OnceCell` to be initialized, and returns a reference
    /// to the value stored.
    pub async fn wait_initialized(&self) -> &T {
        let state = WaitFuture::new(self, false).await;
        debug_assert_eq!(state, STATE_SET);
        // SAFETY: the cell has been initialized
        unsafe { self.get_unchecked() }
    }

    /// Gets the value currently in the `OnceCell`, or initialize it with the
    /// given asynchronous operation.
    ///
    /// If some other task is currently working on initializing the `OnceCell`,
    /// this call will wait for that other task to finish, then return the value
    /// that the other task produced.
    ///
    /// If the provided operation is cancelled or panics, the initialization
    /// attempt is cancelled. If there are other tasks waiting for the value to
    /// be initialized, one of them will start another attempt at initializing
    /// the value.
    ///
    /// This will deadlock if `f` tries to initialize the cell recursively.
    pub async fn get_or_init<F, Fut>(&self, f: F) -> &T
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = T>,
    {
        self.get_or_try_init(|| async move { Ok::<_, Infallible>(f().await) })
            .await
            .unwrap()
    }

    /// Gets the value currently in the `OnceCell`, or initialize it with the
    /// given asynchronous operation.
    ///
    /// If some other task is currently working on initializing the `OnceCell`,
    /// this call will wait for that other task to finish, then return the value
    /// that the other task produced.
    ///
    /// If the provided operation returns an error, is cancelled or panics, the
    /// initialization attempt is cancelled. If there are other tasks waiting
    /// for the value to be initialized, one of them will start another attempt
    /// at initializing the value.
    ///
    /// This will deadlock if `f` tries to initialize the cell recursively.
    pub async fn get_or_try_init<E, F, Fut>(&self, f: F) -> Result<&T, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, E>>,
    {
        crate::trace::async_trace_leaf().await;

        loop {
            // Try to lock the state, using acquire `Acquire` ordering for both success
            // and failure to have happens-before relation with previous initialization
            // attempts.
            match self.state.compare_exchange(
                STATE_UNSET,
                STATE_LOCKED,
                Ordering::Acquire,
                Ordering::Acquire,
            ) {
                // State has been successfully locked,
                // execute the initializer and set the result.
                Ok(_) => {
                    #[cfg(all(tokio_unstable, feature = "tracing"))]
                    self.resource_span.in_scope(|| {
                        tracing::trace!(
                            target: "runtime::resource::state_update",
                            state = "locked",
                        );
                    });
                    // If `f().await` panics, is cancelled, or fails,
                    // the state will be unlocked by the guard.
                    struct DropGuard<'a, T>(&'a OnceCell<T>);
                    impl<T> Drop for DropGuard<'_, T> {
                        fn drop(&mut self) {
                            // Use release ordering to for this failed initialization
                            // attempt to happens-before the next one.
                            self.0.state.store(STATE_UNSET, Ordering::Release);
                            #[cfg(all(tokio_unstable, feature = "tracing"))]
                            self.0.resource_span.in_scope(|| {
                                tracing::trace!(
                                    target: "runtime::resource::state_update",
                                    state = "unset",
                                );
                            });
                            self.0.notify_unlocked();
                        }
                    }
                    let guard = DropGuard(self);
                    let value = f().await?;
                    std::mem::forget(guard);
                    // SAFETY: state has been locked
                    return Ok(unsafe { self.set_value(value) });
                }
                // State is currently locked by another initializer,
                // wait for it to succeed or fail.
                Err(STATE_LOCKED) => {
                    let state = WaitFuture::new(self, true).await;
                    // SAFETY: the cell has been initialized
                    if state == STATE_SET {
                        return Ok(unsafe { self.get_unchecked() });
                    }
                    debug_assert_eq!(state, STATE_UNSET);
                    // The other initializer has failed, try to lock the state again
                    continue;
                }
                // SAFETY: the cell has been initialized
                Err(STATE_SET) => return Ok(unsafe { self.get_unchecked() }),
                // SAFETY: all possible values of state are covered
                Err(_) => unsafe { std::hint::unreachable_unchecked() },
            }
        }
    }

    /// Takes the value from the cell, destroying the cell in the process.
    /// Returns `None` if the cell is empty.
    pub fn into_inner(mut self) -> Option<T> {
        if self.initialized_mut() {
            // Set to uninitialized for the destructor of `OnceCell` to work properly
            self.state.with_mut(|s| *s = STATE_UNSET);
            Some(unsafe { self.value.with(|ptr| ptr::read(ptr).assume_init()) })
        } else {
            None
        }
    }

    /// Takes ownership of the current value, leaving the cell empty. Returns
    /// `None` if the cell is empty.
    pub fn take(&mut self) -> Option<T> {
        std::mem::take(self).into_inner()
    }
}

// Since `get` gives us access to immutable references of the OnceCell, OnceCell
// can only be Sync if T is Sync, otherwise OnceCell would allow sharing
// references of !Sync values across threads. We need T to be Send in order for
// OnceCell to by Sync because we can use `set` on `&OnceCell<T>` to send values
// (of type T) across threads.
unsafe impl<T: Sync + Send> Sync for OnceCell<T> {}

// Access to OnceCell's value is guarded by the semaphore permit
// and atomic operations on `value_set`, so as long as T itself is Send
// it's safe to send it to another thread
unsafe impl<T: Send> Send for OnceCell<T> {}

/// An entry in the wait queue.
struct Waiter {
    /// If the waiter is a cell initializer.
    is_initializer: bool,
    /// Waiting task.
    waker: Option<Waker>,
    /// Intrusive linked-list pointers.
    pointers: linked_list::Pointers<Waiter>,
    /// Should not be `Unpin`.
    _p: PhantomPinned,
}

impl Waiter {
    fn new(is_initializer: bool) -> Self {
        Self {
            is_initializer,
            waker: None,
            pointers: linked_list::Pointers::new(),
            _p: PhantomPinned,
        }
    }
}

// SAFETY: `Waiter` is forced to be !Unpin.
unsafe impl linked_list::Link for Waiter {
    type Handle = NonNull<Waiter>;
    type Target = Waiter;

    fn as_raw(handle: &NonNull<Waiter>) -> NonNull<Waiter> {
        *handle
    }

    unsafe fn from_raw(ptr: NonNull<Waiter>) -> NonNull<Waiter> {
        ptr
    }

    unsafe fn pointers(target: NonNull<Waiter>) -> NonNull<linked_list::Pointers<Waiter>> {
        Waiter::addr_of_pointers(target)
    }
}

generate_addr_of_methods! {
    impl<> Waiter {
        unsafe fn addr_of_pointers(self: NonNull<Self>) -> NonNull<linked_list::Pointers<Waiter>> {
            &self.pointers
        }
    }
}

/// Wait for the cell to be initialized, or the state to be unlocked
/// in the case of an initializer.
struct WaitFuture<'a, T> {
    node: Waiter,
    cell: &'a OnceCell<T>,
    queued: bool,
}

impl<'a, T> WaitFuture<'a, T> {
    /// Initializes the future.
    ///
    /// If `is_initializer` is true, then the future will wait the state
    /// to be unlocked. Otherwise, it waits the cell to be initialized.
    fn new(cell: &'a OnceCell<T>, is_initializer: bool) -> Self {
        Self {
            cell,
            node: Waiter::new(is_initializer),
            queued: false,
        }
    }

    fn project(self: Pin<&mut Self>) -> (Pin<&mut Waiter>, &OnceCell<T>, &mut bool) {
        fn is_unpin<T: Unpin>() {}
        // SAFETY: all fields other than `node` are `Unpin`
        unsafe {
            is_unpin::<&OnceCell<T>>();

            let this = self.get_unchecked_mut();
            (
                Pin::new_unchecked(&mut this.node),
                this.cell,
                &mut this.queued,
            )
        }
    }
}

impl<T> Drop for WaitFuture<'_, T> {
    fn drop(&mut self) {
        // If the future has not been queued, there is no node in the wait list, so we
        // can skip acquiring the lock.
        if !self.queued {
            return;
        }
        // This is where we ensure safety. The future is being dropped,
        // which means we must ensure that the waiter entry is no longer stored
        // in the linked list.
        let mut waiters = self.cell.waiters.lock();

        // remove the entry from the list
        let node = NonNull::from(&mut self.node);
        // SAFETY: we have locked the wait list.
        unsafe { waiters.remove(node) };
    }
}

impl<T> Future for WaitFuture<'_, T> {
    type Output = u8;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let (node, cell, queued) = self.project();
        let coop = ready!(crate::task::coop::poll_proceed(cx));

        match cell.poll_wait(cx, node, *queued) {
            Poll::Ready(state) => {
                coop.made_progress();
                Poll::Ready(state)
            }
            Poll::Pending => {
                *queued = true;
                Poll::Pending
            }
        }
    }
}

/// Errors that can be returned from [`OnceCell::set`].
///
/// [`OnceCell::set`]: crate::sync::OnceCell::set
#[derive(Debug, PartialEq, Eq)]
pub enum SetError<T> {
    /// The cell was already initialized when [`OnceCell::set`] was called.
    ///
    /// [`OnceCell::set`]: crate::sync::OnceCell::set
    AlreadyInitializedError(T),

    /// The cell is currently being initialized.
    InitializingError(T),
}

impl<T> fmt::Display for SetError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetError::AlreadyInitializedError(_) => write!(f, "AlreadyInitializedError"),
            SetError::InitializingError(_) => write!(f, "InitializingError"),
        }
    }
}

impl<T: fmt::Debug> Error for SetError<T> {}

impl<T> SetError<T> {
    /// Whether `SetError` is `SetError::AlreadyInitializedError`.
    pub fn is_already_init_err(&self) -> bool {
        match self {
            SetError::AlreadyInitializedError(_) => true,
            SetError::InitializingError(_) => false,
        }
    }

    /// Whether `SetError` is `SetError::InitializingError`
    pub fn is_initializing_err(&self) -> bool {
        match self {
            SetError::AlreadyInitializedError(_) => false,
            SetError::InitializingError(_) => true,
        }
    }
}
