//! Thread pool for blocking operations

use tokio_sync::oneshot;

use lazy_static::lazy_static;
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Duration;

struct Pool {
    shared: Mutex<Shared>,
    condvar: Condvar,
}

struct Shared {
    queue: VecDeque<Box<dyn FnOnce() + Send>>,
    num_th: u32,
    num_idle: u32,
}

lazy_static! {
    static ref POOL: Pool = Pool::new();
}

const MAX_THREADS: u32 = 1_000;
const KEEP_ALIVE: Duration = Duration::from_secs(10);

/// Run the provided function on a threadpool dedicated to blocking operations.
pub fn run<F, R>(f: F) -> oneshot::Receiver<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();

    let should_spawn = {
        let mut shared = POOL.shared.lock().unwrap();

        shared.queue.push_back(Box::new(move || {
            // The receiver may have dropped
            let _ = tx.send(f());
        }));

        if shared.num_idle == 0 {
            // No threads are able to process the task

            if shared.num_th == MAX_THREADS {
                // At max number of threads
                false
            } else {
                shared.num_th += 1;
                true
            }
        } else {
            shared.num_idle -= 1;
            POOL.condvar.notify_one();
            false
        }
    };

    if should_spawn {
        spawn_thread();
    }

    rx
}

fn spawn_thread() {
    thread::Builder::new()
        .name("tokio-blocking-driver".to_string())
        .spawn(|| {
            'outer: loop {
                let mut shared = POOL.shared.lock().unwrap();

                if let Some(task) = shared.queue.pop_front() {
                    drop(shared);
                    run_task(task);
                    continue;
                }

                // IDLE
                shared.num_idle += 1;

                loop {
                    shared = POOL.condvar.wait_timeout(shared, KEEP_ALIVE).unwrap().0;

                    if let Some(task) = shared.queue.pop_front() {
                        drop(shared);
                        run_task(task);
                        continue 'outer;
                    }
                }
            }
        })
        .unwrap();
}

fn run_task(f: Box<dyn FnOnce() + Send>) {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let _ = catch_unwind(AssertUnwindSafe(|| f()));
}

impl Pool {
    fn new() -> Pool {
        Pool {
            shared: Mutex::new(Shared {
                queue: VecDeque::new(),
                num_th: 0,
                num_idle: 0,
            }),
            condvar: Condvar::new(),
        }
    }
}
