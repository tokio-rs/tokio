//! Thread pool for blocking operations

use tokio_sync::oneshot;

use std::cell::Cell;
use std::collections::VecDeque;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;

#[cfg(feature = "thread-pool")]
mod builder;

#[cfg(feature = "thread-pool")]
pub(crate) use builder::Builder;

#[derive(Clone, Copy)]
enum State {
    Empty,
    Ready(*const Arc<Pool>),
}

thread_local! {
    /// Thread-local tracking the current executor
    static BLOCKING: Cell<State> = Cell::new(State::Empty)
}

/// Set the blocking pool for the duration of the closure
///
/// If a blocking pool is already set, it will be restored when the closure returns or if it
/// panics.
pub(crate) fn with_pool<F, R>(pool: &Arc<Pool>, f: F) -> R
where
    F: FnOnce() -> R,
{
    // While scary, this is safe. The function takes a `&Pool`, which guarantees
    // that the reference lives for the duration of `with_pool`.
    //
    // Because we are always clearing the TLS value at the end of the
    // function, we can cast the reference to 'static which thread-local
    // cells require.
    BLOCKING.with(|cell| {
        let was = cell.replace(State::Empty);

        // Ensure that the pool is removed from the thread-local context
        // when leaving the scope. This handles cases that involve panicking.
        struct Reset<'a>(&'a Cell<State>, State);

        impl Drop for Reset<'_> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }

        let _reset = Reset(cell, was);
        cell.set(State::Ready(pool as *const _));
        f()
    })
}

pub(crate) struct Pool {
    shared: Mutex<Shared>,
    condvar: Condvar,
    new_thread: Box<dyn Fn() -> thread::Builder + Send + Sync + 'static>,
}

impl fmt::Debug for Pool {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Pool").finish()
    }
}

struct Shared {
    queue: VecDeque<Box<dyn FnOnce() + Send>>,
    num_th: u32,
    num_idle: u32,
    num_notify: u32,
}

const MAX_THREADS: u32 = 1_000;
const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Result of a blocking operation running on the blocking thread pool.
#[derive(Debug)]
pub struct Blocking<T> {
    rx: oneshot::Receiver<T>,
}

impl Pool {
    /// Run the provided function on an executor dedicated to blocking operations.
    pub(crate) fn spawn(this: &Arc<Self>, f: Box<dyn FnOnce() + Send + 'static>) {
        let should_spawn = {
            let mut shared = this.shared.lock().unwrap();

            shared.queue.push_back(f);

            if shared.num_idle == 0 {
                // No threads are able to process the task.

                if shared.num_th == MAX_THREADS {
                    // At max number of threads
                    false
                } else {
                    shared.num_th += 1;
                    true
                }
            } else {
                // Notify an idle worker thread. The notification counter
                // is used to count the needed amount of notifications
                // exactly. Thread libraries may generate spurious
                // wakeups, this counter is used to keep us in a
                // consistent state.
                shared.num_idle -= 1;
                shared.num_notify += 1;
                this.condvar.notify_one();
                false
            }
        };

        if should_spawn {
            Arc::clone(this).spawn_thread((this.new_thread)());
        }
    }

    fn spawn_thread(self: Arc<Self>, builder: thread::Builder) {
        builder
            .spawn(move || {
                let mut shared = self.shared.lock().unwrap();
                loop {
                    // BUSY
                    while let Some(task) = shared.queue.pop_front() {
                        drop(shared);
                        run_task(task);
                        shared = self.shared.lock().unwrap();
                    }

                    // IDLE
                    shared.num_idle += 1;

                    loop {
                        let lock_result = self.condvar.wait_timeout(shared, KEEP_ALIVE).unwrap();
                        shared = lock_result.0;
                        let timeout_result = lock_result.1;

                        if shared.num_notify != 0 {
                            // We have received a legitimate wakeup,
                            // acknowledge it by decrementing the counter
                            // and transition to the BUSY state.
                            shared.num_notify -= 1;
                            break;
                        }

                        if timeout_result.timed_out() {
                            // Thread exit
                            shared.num_th -= 1;

                            // num_idle should now be tracked exactly, panic
                            // with a descriptive message if it is not the
                            // case.
                            shared.num_idle = shared
                                .num_idle
                                .checked_sub(1)
                                .expect("num_idle underflowed on thread exit");
                            return;
                        }

                        // Spurious wakeup detected, go back to sleep.
                    }
                }
            })
            .unwrap();
    }
}

/// Run the provided closure on a thread where blocking is acceptable.
///
/// In general, issuing a blocking call or performing a lot of compute in a future without
/// yielding is not okay, as it may prevent the executor from driving other futures forward.
/// A closure that is run through this method will instead be run on a dedicated thread pool for
/// such blocking tasks without holding up the main futures executor.
///
/// # Examples
///
/// ```
/// # async fn docs() {
/// tokio_executor::blocking::run(move || {
///     // do some compute-heavy work or call synchronous code
/// }).await;
/// # }
/// ```
pub fn run<F, R>(f: F) -> Blocking<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();

    BLOCKING.with(|current_pool| match current_pool.get() {
        State::Ready(pool) => {
            let pool = unsafe { &*pool };
            Pool::spawn(
                pool,
                Box::new(move || {
                    // receiver may have gone away
                    let _ = tx.send(f());
                }),
            );
        }
        State::Empty => panic!("must be called from the context of Tokio runtime"),
    });

    Blocking { rx }
}

impl<T> Future for Blocking<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::task::Poll::*;

        match Pin::new(&mut self.rx).poll(cx) {
            Ready(Ok(v)) => Ready(v),
            Ready(Err(_)) => panic!(
                "the blocking operation has been dropped before completing. \
                 This should not happen and is a bug."
            ),
            Pending => Pending,
        }
    }
}

fn run_task(f: Box<dyn FnOnce() + Send>) {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let _ = catch_unwind(AssertUnwindSafe(|| f()));
}

impl Default for Pool {
    fn default() -> Self {
        Pool {
            shared: Mutex::new(Shared {
                queue: VecDeque::new(),
                num_th: 0,
                num_idle: 0,
                num_notify: 0,
            }),
            condvar: Condvar::new(),
            new_thread: Box::new(|| {
                thread::Builder::new().name("tokio-blocking-driver".to_string())
            }),
        }
    }
}
