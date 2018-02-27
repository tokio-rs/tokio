use reactor::{Handle, Direction};

use futures::{Async, Poll};
use futures::task::{self, Task};
use mio::{self, Evented};

use std::{io, mem, usize};
use std::cell::UnsafeCell;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::SeqCst;

/// Handle to a reactor registration.
///
/// A registration represents an I/O resource registered with a Reactor such
/// that it will receive task notifications on readiness.
///
/// The registration is lazily made and supports concurrent operations. This
/// allows a `Registration` instance to be created without the reactor handle
/// that will eventually be used to drive the resource.
///
/// The difficulty is due to the fact that a single registration drives two
/// separate tasks -- A read half and a write half.
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
    handle: Handle,
    token: usize,
}

/// Tasks waiting on readiness notifications.
#[derive(Debug)]
struct Node {
    direction: Direction,
    task: Task,
    next: Option<Box<Node>>,
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
    /// If the registration happened successfully, `Ok(true)` is returned.
    ///
    /// If an I/O resource has previously been successfully registered,
    /// `Ok(false)` is returned.
    ///
    /// If an error is encountered during registration, `Err` is returned.
    pub fn register<T>(&self, io: &T) -> io::Result<bool>
    where T: Evented,
    {
        self.register2(io, || Handle::try_current())
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
    where T: Evented,
    {
        self.register2(io, || Ok(handle.clone()))
    }

    fn register2<T, F>(&self, io: &T, f: F) -> io::Result<bool>
    where T: Evented,
          F: Fn() -> io::Result<Handle>,
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

                    unsafe { *self.inner.get() = Some(inner); }

                    // Transition out of the locked state. This acquires the
                    // current value, potentially having a list of tasks that
                    // are pending readiness notifications.
                    let actual = self.state.swap(READY, SeqCst);

                    // Consume the stack of nodes.
                    let ptr = actual & !LIFECYCLE_MASK;

                    if ptr != 0 {
                        let mut read = false;
                        let mut write = false;
                        let mut curr = unsafe { Box::from_raw(ptr as *mut Node) };

                        let inner = unsafe { (*self.inner.get()).as_ref().unwrap() };

                        loop {
                            let node = *curr;
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

                            match next {
                                Some(next) => curr = next,
                                None => break,
                            }
                        }
                    }

                    return res.map(|_| true);
                }
                _ => return Ok(false),
            }
        }
    }

    /// Poll for changes in the I/O resource's read readiness.
    pub fn poll_read_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.poll_ready(Direction::Read, true)
            .map(|v| match v {
                Some(v) => Async::Ready(v),
                _ => Async::NotReady,
            })
    }

    /// Try taking the I/O resource's read readiness.
    ///
    /// Unlike `poll_read_ready`, this does not register the current task for
    /// notification.
    pub fn take_read_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Read, false)

    }

    /// Poll for changes in the I/O resource's write readiness.
    pub fn poll_write_ready(&self) -> Poll<mio::Ready, io::Error> {
        self.poll_ready(Direction::Write, true)
            .map(|v| match v {
                Some(v) => Async::Ready(v),
                _ => Async::NotReady,
            })
    }

    /// Try taking the I/O resource's write readiness.
    ///
    /// Unlike `poll_write_ready`, this does not register the current task for
    /// notification.
    pub fn take_write_ready(&self) -> io::Result<Option<mio::Ready>> {
        self.poll_ready(Direction::Write, false)
    }

    fn poll_ready(&self, direction: Direction, notify: bool)
        -> io::Result<Option<mio::Ready>>
    {
        let mut state = self.state.load(SeqCst);

        // Cache the node pointer
        let mut node = None;

        loop {
            match state {
                INIT => {
                    return Err(io::Error::new(io::ErrorKind::Other, "must call `register`
                                              before poll_read_ready"));
                }
                READY => {
                    let inner = unsafe { (*self.inner.get()).as_ref().unwrap() };
                    return inner.poll_ready(direction, notify);
                }
                _ => {
                    if !notify {
                        // Skip the notification tracking junk.
                        return Ok(None);
                    }

                    let ptr = state & !LIFECYCLE_MASK;

                    // Get the node
                    let mut n = node.take().unwrap_or_else(|| {
                        Box::new(Node {
                            direction,
                            task: task::current(),
                            next: None,
                        })
                    });

                    n.next = if ptr == 0 {
                        None
                    } else {
                        // Great care must be taken of the CAS fails
                        Some(unsafe { Box::from_raw(ptr as *mut Node) })
                    };

                    let ptr = Box::into_raw(n);
                    let next = ptr as usize | (state & LIFECYCLE_MASK);

                    let actual = self.state.compare_and_swap(state, next, SeqCst);

                    if actual != state {
                        // Back out of the node boxing
                        let mut n = unsafe { Box::from_raw(ptr) };

                        // We don't really own this
                        mem::forget(n.next.take());

                        // Save this for next loop
                        node = Some(n);

                        state = actual;
                        continue;
                    }

                    return Ok(None);
                }
            }
        }
    }
}

unsafe impl Send for Registration {}
unsafe impl Sync for Registration {}

// ===== impl Inner =====

impl Inner {
    fn new<T>(io: &T, handle: Handle) -> (Self, io::Result<()>)
    where T: Evented,
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

        let inner = Inner {
            handle,
            token,
        };

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

    fn poll_ready(&self, direction: Direction, notify: bool)
        -> io::Result<Option<mio::Ready>>
    {
        if self.token == ERROR {
            return Err(io::Error::new(io::ErrorKind::Other, "failed to associate with reactor"));
        }

        let inner = match self.handle.inner() {
            Some(inner) => inner,
            None => return Err(io::Error::new(io::ErrorKind::Other, "reactor gone")),
        };

        let mask = direction.mask();

        let io_dispatch = inner.io_dispatch.read().unwrap();
        let sched = &io_dispatch[self.token];

        let mut ready = mask & sched.readiness.fetch_and(!mask, SeqCst);

        if ready == 0 && notify {
            // Update the task info
            match direction {
                Direction::Read => sched.reader.register(),
                Direction::Write => sched.writer.register(),
            }

            // Try again
            ready = mask & sched.readiness.fetch_and(!mask, SeqCst);
        }

        if ready == 0 {
            Ok(None)
        } else {
            Ok(Some(super::usize2ready(ready)))
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
