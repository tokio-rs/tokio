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
    queue: VecDeque<Box<dyn Task>>,
    num_th: u32,
    num_idle: u32,
}

lazy_static! {
    static ref POOL: Pool = Pool::new();
}

trait Task: Send {
    fn run(self: Box<Self>);
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
            tx.send(f()).ok().unwrap();
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
        .name("fs-driver".to_string())
        .spawn(|| {
            'outer: loop {
                let mut shared = POOL.shared.lock().unwrap();

                if let Some(task) = shared.queue.pop_front() {
                    drop(shared);
                    task.run();
                    continue;
                }

                // IDLE
                shared.num_idle += 1;

                loop {
                    shared = POOL.condvar.wait_timeout(shared, KEEP_ALIVE).unwrap().0;

                    if let Some(task) = shared.queue.pop_front() {
                        drop(shared);
                        task.run();
                        continue 'outer;
                    }
                }
            }
        })
        .unwrap();
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

impl<F: FnOnce() + Send> Task for F {
    fn run(self: Box<Self>) {
        self();
    }
}
