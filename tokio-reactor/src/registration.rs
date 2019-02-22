use {Direction, Handle, HandlePriv, Task};

use futures::{task, Async, Poll};
use mio::{self, Evented};

use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;
use std::{io, ptr, usize};

/// Associates an I/O resource with the reactor instance that drives it.
///
/// A registration represents an I/O resource registered with a Reactor such
/// that it will receive task notifications on readiness. This is the lowest
/// level API for integrating with a reactor.
///
/// The association between an I/O resource is made by calling [`register`].
/// Once the association is established, it remains established until the
/// registration instance is dropped. Subsequent calls to [`register`] are
/// no-ops.
///
/// A registration instance represents two separate readiness streams. One for
/// the read readiness and one for write readiness. These streams are
/// independent and can be consumed from separate tasks.
///
/// **Note**: while `Registration` is `Sync`, the caller must ensure that there
/// are at most two tasks that use a registration instance concurrently. One
/// task for [`poll_read_ready`] and one task for [`poll_write_ready`]. While
/// violating this requirement is "safe" from a Rust memory safety point of
/// view, it will result in unexpected behavior in the form of lost
/// notifications and tasks hanging.
///
/// ## Platform-specific events
///
/// `Registration` also allows receiving platform-specific `mio::Ready` events.
/// These events are included as part of the read readiness event stream. The
/// write readiness event stream is only for `Ready::writable()` events.
///
/// [`register`]: #method.register
/// [`poll_read_ready`]: #method.poll_read_ready`]
/// [`poll_write_ready`]: #method.poll_write_ready`]
#[derive(Debug)]
pub struct Registration {
    /// Stores the handle. Once set, the value is not changed.
    ///
    /// Setting this requires acquiring the lock from state.
    inner: UnsafeCell<Option<Inner>>,

    /// Tracks the state of the registration.
    ///
    /// The least significant 2 bits are used to track the lifecycle of the
    /// registration. The rest of the `state` variable is a pointer to tasks
    /// that must be notified once the lock is released.
    state: AtomicUsize,
}

#[derive(Debug)]
struct Inner {
    handle: HandlePriv,
    token: usize,
}

#[derive(PartialEq)]
enum Notify {
    Yes,
    No,
}

/// Tasks waiting on readiness notifications.
#[derive(Debug)]
struct Node {
    direction: Direction,
    task: Task,
    next: *mut Node,
}

/// Initial state. The handle is not set and the registration is idle.
const INIT: usize = 0;

/// A thread locked the state and will associate a handle.
const LOCKED: usize = 1;

/// A handle has been associated with the registration.
const READY: usize = 2;

/// Masks the lifecycle state
const LIFECYCLE_MASK: usize = 0b11;

/// A fake token used to identify error situations
const ERROR: usize = usize::MAX;

// ===== impl Registration =====

impl Registration {
    /// Create a new `Registration`.
    ///
    /// This registration is not associated with a Reactor instance. Call
    /// `register` to establish the association.
    pub fn new() -> Registration {
        Registration {
            inner: UnsafeCell::new(None),
            state: AtomicUsize::new(INIT),
        }
    }

    /// Register the I/O resource with the default reactor.
    ///
    /// This function is safe to call concurrently and repeatedly. However, only
    /// the first call will establish the registration. Subsequent calls will be
    /// no-ops.
    ///
    /// # Return
    ///
    /// If the registration happened successfully, `Ok(true)` is returned.
    ///
    /// If an I/O resource has previously been successfully registered,
    /// `Ok(false)` is returned.
    ///
    /// If an error is encountered during registration, `Err` is returned.
    pub fn register<T>(&self, io: &T) -> io::Result<bool>
    where
        T: Evented,
    {
        self.register2(io, || HandlePriv::try_current())
    }

    /// Deregister the I/O resource from the reactor it is associated with.
    ///
    /// This function must be called before the I/O resource associated with the
    /// registration is dropped.
    ///
    /// Note that deregistering does not guarantee that the I/O resource can be
    /// registered with a different reactor. Some I/O resource types can only be
    /// associated with a single reactor instance for their lifetime.
    ///
    /// # Return
    ///
    /// If the deregistration was successful, `Ok` is returned. Any calls to
    /// `Reactor::turn` that happen after a successful call to `deregister` will
    /// no longer result in notifications getting sent for this registration.
    ///
    /// `Err` is returned if an error is encountered.
    pub fn deregister<T>(&mut self, io: &T) -> io::Result<()>
    where
        T: Evented,
    {
        // The state does not need to be checked and coordination is not
        // necessary as this function takes `&mut self`. This guarantees a
        // single thread is accessing the instance.
        if let Some(inner) = unsafe { (*self.inner.get()).as_ref() } {
            inner.deregister(io)?;
        }

        Ok(())
    }

    /// Register the I/O resource with the specified reactor.
    ///
    /// This function is safe to call concurrently and repeatedly. However, only
    /// the first call will establish the registration. Subsequent calls will be
    /// no-ops.
    ///
    /// If the registration happened successfully, `Ok(true)` is returned.
    ///
    /// If an I/O resource has previously been successfully registered,
    /// `Ok(false)` is returned.
    ///
    /// If an error is encountered during registration, `Err` is returned.
    pub fn register_with<T>(&self, io: &T, handle: &Handle) -> io::Result<bool>
    where
        T: Evented,
    {
        self.register2(io, || match handle.as_priv() {
            Some(handle) => Ok(handle.clone()),
            None => HandlePriv::try_current(),
        })
    }

    pub(crate) fn register_with_priv<T>(&self, io: &T, handle: &HandlePriv) -> io::Result<bool>
    where
        T: Evented,
    {
        self.register2(io, || Ok(handle.clone()))
    }

    fn register2<T, F>(&self, io: &T, f: F) -> io::Result<bool>
    where
        T: Evented,
        F: Fn() -> io::Result<HandlePriv>,
    {
        let mut state = self.state.load(SeqCst);

        loop {
            match state {
                INIT => {
                    // Registration is currently not associated with a handle.
                    // Get a handle then attempt to lock the state.
                    let handle = f()?;

                    let actual = self.state.compare_and_swap(INIT, LOCKED, SeqCst);

                    if actual != state {
                        state = actual;
                        continue;
                    }

                    // Create the actual registration
                    let (inner, res) = Inner::new(io, handle);

                    unsafe {
                        *self.inner.get() = Some(inner);
                    }

                    // Transition out of the locked state. This acquires the
                    // current value, potentially having a list of tasks that
                    // are pending readiness notifications.
                    let actual = self.state.swap(READY, SeqCst);

                    // Consume the stack of nodes

                    let mut read = false;
                    let mut write = false;
                    let mut ptr = (actual & !LIFECYCLE_MASK) as *mut Node;

                    let inner = unsafe { (*self.inner.get()).as_ref().unwrap() };

                    while !ptr.is_null() {
                        let node = unsafe { Box::from_raw(ptr) };
                        let node = *node;
                        let Node {
                            direction,
                            task,
                            next,
                        } = node;

                        let flag = match direction {
                            Direction::Read => &mut read,
                            Direction::Write => &mut write,
                        };

                        if !*flag {
                            *flag = true;

                            inner.register(direction, task);
                        }

                        ptr = next;
                    }

                    return res.map(|_| true);
                }
                _ => return Ok(false),
            }
        }
    }

    /// Poll for events on the I/O resource's read readiness stream.
    ///
    /// If the I/O resource receives a new read readiness event since the last
    /// call to `poll_read_ready`, it is returned. If it has not, the current
    /// task is notified once a new event is received.
    ///
    /// All events except `HUP` are [edge-triggered]. Once `HUP` is returned,
    /// the function will always return `Ready(HUP)`. This should be treated as
    /// the end of the readiness stream.
    ///
    /// Ensure that [`register`] has been called first.
    ///
    /// # Return value
    ///
    /// There are several possible return values:
    ///
    /// * `Ok(Async::Ready(readiness))` means that the I/O resource has received
    ///   a new readiness event. The readiness value is included.
    ///
    /// * `Ok(NotReady)` means that no new readiness events have been received
    ///   since the last call to `poll_read_ready`.
    ///
    /// * `Err(err)` means that the registration has encountered an error. This
    ///   error either represents a permanent internal error **or** the fact
    ///   that [`register`] was not called first.
    ///
    /// [`register`]: #method.register
    /// [edge-triggered]: https://docs.rs/mio/0.6/mio/struct.Poll.html#edge-triggered-and-level-triggered
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_read_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.poll_ready(Direction::Read, Notify::Yes)
            .map(|v| match v {
                Some(v) => Async::Ready(v),
                _ => Async::NotReady,
            })
    }

    /// Consume any pending read readiness event.
    ///
    /// This function is identical to [`poll_read_ready`] **except** that it
    /// will not notify the current task when a new event is received. As such,
    /// it is safe to call this function from outside of a task context.
    ///
    /// [`poll_read_ready`]: #method.poll_read_ready
    pub fn take_read_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Read, Notify::No)
    }

    /// Poll for events on the I/O resource's write readiness stream.
    ///
    /// If the I/O resource receives a new write readiness event since the last
    /// call to `poll_write_ready`, it is returned. If it has not, the current
    /// task is notified once a new event is received.
    ///
    /// All events except `HUP` are [edge-triggered]. Once `HUP` is returned,
    /// the function will always return `Ready(HUP)`. This should be treated as
    /// the end of the readiness stream.
    ///
    /// Ensure that [`register`] has been called first.
    ///
    /// # Return value
    ///
    /// There are several possible return values:
    ///
    /// * `Ok(Async::Ready(readiness))` means that the I/O resource has received
    ///   a new readiness event. The readiness value is included.
    ///
    /// * `Ok(NotReady)` means that no new readiness events have been received
    ///   since the last call to `poll_write_ready`.
    ///
    /// * `Err(err)` means that the registration has encountered an error. This
    ///   error either represents a permanent internal error **or** the fact
    ///   that [`register`] was not called first.
    ///
    /// [`register`]: #method.register
    /// [edge-triggered]: https://docs.rs/mio/0.6/mio/struct.Poll.html#edge-triggered-and-level-triggered
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    pub fn poll_write_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.poll_ready(Direction::Write, Notify::Yes)
            .map(|v| match v {
                Some(v) => Async::Ready(v),
                _ => Async::NotReady,
            })
    }

    /// Consume any pending write readiness event.
    ///
    /// This function is identical to [`poll_write_ready`] **except** that it
    /// will not notify the current task when a new event is received. As such,
    /// it is safe to call this function from outside of a task context.
    ///
    /// [`poll_write_ready`]: #method.poll_write_ready
    pub fn take_write_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Write, Notify::No)
    }

    fn poll_ready(&self, direction: Direction, notify: Notify) -> io::Result<Option<mio::Ready>> {
        let mut state = self.state.load(SeqCst);

        // Cache the node pointer
        let mut node = None;

        loop {
            match state {
                INIT => {
                    return Err(io::Error::new(
                        io::ErrorKind::Other,
                        "must call `register`
                                              before poll_read_ready",
                    ));
                }
                READY => {
                    let inner = unsafe { (*self.inner.get()).as_ref().unwrap() };
                    return inner.poll_ready(direction, notify);
                }
                LOCKED => {
                    if let Notify::No = notify {
                        // Skip the notification tracking junk.
                        return Ok(None);
                    }

                    let next_ptr = (state & !LIFECYCLE_MASK) as *mut Node;

                    let task = task::current();

                    // Get the node
                    let mut n = node.take().unwrap_or_else(|| {
                        Box::new(Node {
                            direction,
                            task: task,
                            next: ptr::null_mut(),
                        })
                    });

                    n.next = next_ptr;

                    let node_ptr = Box::into_raw(n);
                    let next = node_ptr as usize | (state & LIFECYCLE_MASK);

                    let actual = self.state.compare_and_swap(state, next, SeqCst);

                    if actual != state {
                        // Back out of the node boxing
                        let n = unsafe { Box::from_raw(node_ptr) };

                        // Save this for next loop
                        node = Some(n);

                        state = actual;
                        continue;
                    }

                    return Ok(None);
                }
                _ => unreachable!(),
            }
        }
    }
}

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

// ===== impl Inner =====

impl Inner {
    fn new<T>(io: &T, handle: HandlePriv) -> (Self, io::Result<()>)
    where
        T: Evented,
    {
        let mut res = Ok(());

        let token = match handle.inner() {
            Some(inner) => match inner.add_source(io) {
                Ok(token) => token,
                Err(e) => {
                    res = Err(e);
                    ERROR
                }
            },
            None => {
                res = Err(io::Error::new(io::ErrorKind::Other, "event loop gone"));
                ERROR
            }
        };

        let inner = Inner { handle, token };

        (inner, res)
    }

    fn register(&self, direction: Direction, task: Task) {
        if self.token == ERROR {
            task.notify();
            return;
        }

        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => {
                task.notify();
                return;
            }
        };

        inner.register(self.token, direction, task);
    }

    fn deregister<E: Evented>(&self, io: &E) -> io::Result<()> {
        if self.token == ERROR {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to associate with reactor",
            ));
        }

        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        inner.deregister_source(io)
    }

    fn poll_ready(&self, direction: Direction, notify: Notify) -> io::Result<Option<mio::Ready>> {
        if self.token == ERROR {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "failed to associate with reactor",
            ));
        }

        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        let mask = direction.mask();
        let mask_no_hup = (mask - ::platform::hup()).as_usize();

        let io_dispatch = inner.io_dispatch.read();
        let sched = &io_dispatch[self.token];

        // This consumes the current readiness state **except** for HUP. HUP is
        // excluded because a) it is a final state and never transitions out of
        // HUP and b) both the read AND the write directions need to be able to
        // observe this state.
        //
        // If HUP were to be cleared when `direction` is `Read`, then when
        // `poll_ready` is called again with a _`direction` of `Write`, the HUP
        // state would not be visible.
        let mut ready =
            mask & mio::Ready::from_usize(sched.readiness.fetch_and(!mask_no_hup, SeqCst));

        if ready.is_empty() && notify == Notify::Yes {
            debug!("scheduling {:?} for: {}", direction, self.token);
            // Update the task info
            match direction {
                Direction::Read => sched.reader.register(),
                Direction::Write => sched.writer.register(),
            }

            // Try again
            ready = mask & mio::Ready::from_usize(sched.readiness.fetch_and(!mask_no_hup, SeqCst));
        }

        if ready.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ready))
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        if self.token == ERROR {
            return;
        }

        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return,
        };

        inner.drop_source(self.token);
    }
}
