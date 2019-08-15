use std::cell::Cell;
use std::error::Error;
use std::fmt;
use std::prelude::v1::*;

use futures::{self, Future};

thread_local!(static ENTERED: Cell<bool> = Cell::new(false));

/// Represents an executor context.
///
/// For more details, see [`enter` documentation](fn.enter.html)
pub struct Enter {
    on_exit: Vec<Box<dyn Callback>>,
    permanent: bool,
}

/// An error returned by `enter` if an execution scope has already been
/// entered.
pub struct EnterError {
    _a: (),
}

impl fmt::Debug for EnterError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("EnterError")
            .field("reason", &self.description())
            .finish()
    }
}

impl fmt::Display for EnterError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.description())
    }
}

impl Error for EnterError {
    fn description(&self) -> &str {
        "attempted to run an executor while another executor is already running"
    }
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
            })
        }
    })
}

// Forces the current "entered" state to be cleared while the closure
// is executed.
//
// # Warning
//
// This is hidden for a reason. Do not use without fully understanding
// executors. Misuing can easily cause your program to deadlock.
#[doc(hidden)]
pub fn exit<F: FnOnce() -> R, R>(f: F) -> R {
    // Reset in case the closure panics
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ENTERED.with(|c| {
                c.set(true);
            });
        }
    }

    ENTERED.with(|c| {
        debug_assert!(c.get());
        c.set(false);
    });

    let reset = Reset;
    let ret = f();
    ::std::mem::forget(reset);

    ENTERED.with(|c| {
        assert!(!c.get(), "closure claimed permanent executor");
        c.set(true);
    });

    ret
}

impl Enter {
    /// Register a callback to be invoked if and when the thread
    /// ceased to act as an executor.
    pub fn on_exit<F>(&mut self, f: F)
    where
        F: FnOnce() + 'static,
    {
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

    /// Blocks the thread on the specified future, returning the value with
    /// which that future completes.
    pub fn block_on<F: Future>(&mut self, f: F) -> Result<F::Item, F::Error> {
        futures::executor::spawn(f).wait_future()
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
                return;
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
