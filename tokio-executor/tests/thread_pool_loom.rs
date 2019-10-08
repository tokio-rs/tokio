#![cfg(loom)]

macro_rules! thread_local {
    ($($tts:tt)+) => { loom::thread_local!{ $($tts)+ } }
}

macro_rules! dbg {
    ($($t:tt)*) => {
        $($t)*
        // Uncomment this line to get a _lot_ of debug output
        // std::dbg!($($t)*)
    }
}

#[path = "../src/enter.rs"]
#[allow(warnings)]
mod enter;
use enter::enter;

#[path = "../src/global.rs"]
#[allow(warnings)]
mod global;
use global::{spawn, with_default};

#[path = "../src/park/mod.rs"]
#[allow(warnings)]
mod park;

#[path = "../src/thread_pool/mod.rs"]
#[allow(warnings)]
mod thread_pool;
use thread_pool::ThreadPool;

#[path = "../src/task/mod.rs"]
#[allow(warnings)]
mod task;

#[path = "../src/util/mod.rs"]
#[allow(warnings)]
mod util;

use tokio_executor::{Executor, TypedExecutor, SpawnError};

mod loom {
    pub(crate) use loom::*;

    pub(crate) mod rand {
        pub(crate) fn seed() -> u64 {
            1
        }
    }

    pub(crate) mod sys {
        pub(crate) fn num_cpus() -> usize {
            2
        }
    }
}
use crate::loom::sync::{Arc, Mutex};
use crate::loom::sync::atomic::{AtomicUsize, AtomicBool};
use crate::loom::sync::atomic::Ordering::{Acquire, Relaxed, Release};

use std::future::Future;

mod support {
    pub mod loom_oneshot;
}
use support::loom_oneshot as oneshot;

#[test]
fn multi_spawn() {
    loom::model(|| {
        let pool = ThreadPool::new();

        let c1 = Arc::new(AtomicUsize::new(0));

        let (tx, rx) = oneshot::channel();
        let tx1 = Arc::new(Mutex::new(Some(tx)));

        // Spawn a task
        let c2 = c1.clone();
        let tx2 = tx1.clone();
        pool.spawn(async move {
            spawn(async move {
                if 1 == c1.fetch_add(1, Relaxed) {
                    tx1.lock().unwrap().take().unwrap().send(());
                }
            });
        });

        // Spawn a second task
        pool.spawn(async move {
            spawn(async move {
                if 1 == c2.fetch_add(1, Relaxed) {
                    tx2.lock().unwrap().take().unwrap().send(());
                }
            });
        });

        rx.recv();
    });
}

#[test]
fn multi_notify() {
    loom::model(|| {
        let pool = ThreadPool::new();

        let c1 = Arc::new(AtomicUsize::new(0));

        let (done_tx, done_rx) = oneshot::channel();
        let done_tx1 = Arc::new(Mutex::new(Some(done_tx)));

        // Spawn a task
        let c2 = c1.clone();
        let done_tx2 = done_tx1.clone();
        pool.spawn(async move {
            gated().await;
            gated().await;

            if 1 == c1.fetch_add(1, Relaxed) {
                done_tx1.lock().unwrap().take().unwrap().send(());
            }
        });

        // Spawn a second task
        pool.spawn(async move {
            gated().await;
            gated().await;

            if 1 == c2.fetch_add(1, Relaxed) {
                done_tx2.lock().unwrap().take().unwrap().send(());
            }
        });

        done_rx.recv();
    });
}

fn gated() -> impl Future<Output = &'static str> {
    use futures_util::future::poll_fn;
    use std::sync::Arc;
    use std::task::Poll;

    let gate = Arc::new(AtomicBool::new(false));
    let mut fired = false;

    poll_fn(move |cx| {
        if !fired {
            let gate = gate.clone();
            let waker = cx.waker().clone();

            spawn(async move {
                gate.store(true, Release);
                waker.wake_by_ref();
            });

            fired = true;

            return Poll::Pending;
        }

        if gate.load(Acquire) {
            Poll::Ready("hello world")
        } else {
            Poll::Pending
        }
    })
}
