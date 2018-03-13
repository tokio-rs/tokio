use std::prelude::v1::*;
use std::cell::Cell;
use std::fmt;

#[cfg(feature = "unstable-futures")]
use futures2;

thread_local!(static ENTERED: Cell<bool> = Cell::new(false));

/// Represents an executor context.
///
/// For more details, see [`enter` documentation](fn.enter.html)
pub struct Enter {
    on_exit: Vec<Box<Callback>>,
    permanent: bool,

    #[cfg(feature = "unstable-futures")]
    _enter2: futures2::executor::Enter,
}

/// An error returned by `enter` if an execution scope has already been
/// entered.
#[derive(Debug)]
pub struct EnterError {
    _a: (),
}

/// Marks the current thread as being within the dynamic extent of an
/// executor.
///
/// Executor implementations should call this function before blocking the
/// thread. If `None` is returned, the executor should fail by panicking or
/// taking some other action without blocking the current thread. This prevents
/// deadlocks due to multiple executors competing for the same thread.
///
/// # Error
///
/// Returns an error if the current thread is already marked
pub fn enter() -> Result<Enter, EnterError> {
    ENTERED.with(|c| {
        if c.get() {
            Err(EnterError { _a: () })
        } else {
            c.set(true);

            Ok(Enter {
                on_exit: Vec::new(),
                permanent: false,

                #[cfg(feature = "unstable-futures")]
                _enter2: futures2::executor::enter().unwrap(),
            })
        }
    })
}

impl Enter {
    /// Register a callback to be invoked if and when the thread
    /// ceased to act as an executor.
    pub fn on_exit<F>(&mut self, f: F) where F: FnOnce() + 'static {
        self.on_exit.push(Box::new(f));
    }

    /// Treat the remainder of execution on this thread as part of an
    /// executor; used mostly for thread pool worker threads.
    ///
    /// All registered `on_exit` callbacks are *dropped* without being
    /// invoked.
    pub fn make_permanent(mut self) {
        self.permanent = true;
    }
}

impl fmt::Debug for Enter {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Enter").finish()
    }
}

impl Drop for Enter {
    fn drop(&mut self) {
        ENTERED.with(|c| {
            assert!(c.get());

            if self.permanent {
                return
            }

            for callback in self.on_exit.drain(..) {
                callback.call();
            }

            c.set(false);
        });
    }
}

trait Callback: 'static {
    fn call(self: Box<Self>);
}

impl<F: FnOnce() + 'static> Callback for F {
    fn call(self: Box<Self>) {
        (*self)()
    }
}
