use crate::future::poll_fn;
use crate::platform::linux::uring::driver::{Close, Op};

use std::cell::RefCell;
use std::os::unix::io::{FromRawFd, RawFd};
use std::rc::Rc;
use std::task::Waker;

// Tracks in-flight operations on a file descriptor. Ensures all in-flight
// operations complete before submitting the close.
#[derive(Clone)]
pub(crate) struct SharedFd {
    inner: Rc<Inner>,
}

unsafe impl Send for SharedFd {}
unsafe impl Sync for SharedFd {}

struct Inner {
    // Open file descriptor
    fd: RawFd,

    // Waker to notify when the close operation completes.
    state: RefCell<State>,
}

enum State {
    /// Initial state
    Init,

    /// Waiting for all in-flight operation to complete.
    Waiting(Option<Waker>),

    /// The FD is closing
    Closing(Op<Close>),

    /// The FD is fully closed
    Closed,
}

impl SharedFd {
    pub(crate) fn new(fd: RawFd) -> SharedFd {
        SharedFd {
            inner: Rc::new(Inner {
                fd,
                state: RefCell::new(State::Init),
            }),
        }
    }

    /// Returns the RawFd
    pub(crate) fn raw_fd(&self) -> RawFd {
        self.inner.fd
    }

    /// An FD cannot be closed until all in-flight operation have completed.
    /// This prevents bugs where in-flight reads could operate on the incorrect
    /// file descriptor.
    ///
    /// TO model this, if there are no in-flight operations, then
    pub(crate) async fn close(mut self) {
        // Get a mutable reference to Inner, indicating there are no
        // in-flight operations on the FD.
        if let Some(inner) = Rc::get_mut(&mut self.inner) {
            // Submit the close operation
            inner.submit_close_op();
        }

        self.inner.closed().await;
    }
}

impl Inner {
    /// If there are no in-flight operations, submit the operation.
    fn submit_close_op(&mut self) {
        // Close the FD
        let state = RefCell::get_mut(&mut self.state);

        // Submit a close operation
        *state = match Op::close(self.fd) {
            Ok(op) => State::Closing(op),
            Err(_) => {
                // Submitting the operation failed, we fall back on a
                // synchronous `close`. This is safe as, at this point, we
                // guarantee all in-flight operations have completed. The most
                // common cause for an error is attempting to close the FD while
                // off runtime.
                //
                // This is done by initializing a `File` with the FD and
                // dropping it.
                //
                // TODO: Should we warn?
                let _ = unsafe { std::fs::File::from_raw_fd(self.fd) };

                State::Closed
            }
        };
    }

    /// Completes when the FD has been closed.
    async fn closed(&self) {
        use std::future::Future;
        use std::pin::Pin;
        use std::task::Poll;

        poll_fn(|cx| {
            let mut state = self.state.borrow_mut();

            match &mut *state {
                State::Init => {
                    *state = State::Waiting(Some(cx.waker().clone()));
                    Poll::Pending
                }
                State::Waiting(Some(waker)) => {
                    if !waker.will_wake(cx.waker()) {
                        *waker = cx.waker().clone();
                    }

                    Poll::Pending
                }
                State::Waiting(None) => {
                    *state = State::Waiting(Some(cx.waker().clone()));
                    Poll::Pending
                }
                State::Closing(op) => {
                    // Nothing to do if the close opeation failed.
                    let _ = ready!(Pin::new(op).poll(cx));
                    *state = State::Closed;
                    Poll::Ready(())
                }
                State::Closed => Poll::Ready(()),
            }
        })
        .await;
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        // Submit the close operation, if needed
        match RefCell::get_mut(&mut self.state) {
            State::Init | State::Waiting(..) => {
                self.submit_close_op();
            }
            _ => {}
        }
    }
}
