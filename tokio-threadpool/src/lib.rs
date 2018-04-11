//! A work-stealing based thread pool for executing futures.

#![doc(html_root_url = "https://docs.rs/tokio-threadpool/0.1.2")]
#![deny(warnings, missing_docs, missing_debug_implementations)]

extern crate tokio_executor;
extern crate futures;
extern crate crossbeam_deque as deque;
extern crate num_cpus;
extern crate rand;

#[macro_use]
extern crate log;

#[cfg(feature = "unstable-futures")]
extern crate futures2;

pub mod park;

mod builder;
mod callback;
mod config;
#[cfg(feature = "unstable-futures")]
mod futures2_wake;
mod notifier;
mod pool;
mod sender;
mod shutdown;
mod shutdown_task;
mod task;
mod thread_pool;
mod worker;

pub use builder::Builder;
pub use sender::Sender;
pub use shutdown::Shutdown;
pub use thread_pool::ThreadPool;
pub use worker::Worker;

#[cfg(not(feature = "unstable-futures"))]
#[cfg(test)]
mod tests {
    // We want most tests as integration tests, but these ones are special:
    //
    // This is testing that Task drop never happens more than it should,
    // causing use-after-free bugs. ;_;

    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::{Relaxed, Release};
    use super::{Sender, Shutdown, ThreadPool};

    #[cfg(not(feature = "unstable-futures"))]
    use futures::{Poll, Async, Future};

    static TASK_DROPS: AtomicUsize = ::std::sync::atomic::ATOMIC_USIZE_INIT;

    fn reset_task_drops() {
        TASK_DROPS.store(0, Release);
    }

    fn spawn_pool<F>(pool: &mut Sender, f: F)
        where F: Future<Item = (), Error = ()> + Send + 'static
    {
        pool.spawn(f).unwrap()
    }

    fn await_shutdown(shutdown: Shutdown) {
        shutdown.wait().unwrap()
    }

    #[test]
    fn task_drop_counts() {
        extern crate env_logger;
        let _ = env_logger::init();

        struct Always;

        impl Future for Always {
            type Item = ();
            type Error = ();

            fn poll(&mut self) -> Poll<(), ()> {
                Ok(Async::Ready(()))
            }
        }

        reset_task_drops();

        let pool = ThreadPool::new();
        let mut tx = pool.sender().clone();
        spawn_pool(&mut tx, Always);
        await_shutdown(pool.shutdown());

        // We've never cloned the waker/notifier, so should only be 1 drop
        assert_eq!(TASK_DROPS.load(Relaxed), 1);


        struct Park;

        impl Future for Park {
            type Item = ();
            type Error = ();

            fn poll(&mut self) -> Poll<(), ()> {
                ::futures::task::current().notify();
                Ok(Async::Ready(()))
            }
        }

        reset_task_drops();

        let pool = ThreadPool::new();
        let mut tx = pool.sender().clone();
        spawn_pool(&mut tx, Park);
        await_shutdown(pool.shutdown());

        // We've cloned the task once, so should be 2 drops
        assert_eq!(TASK_DROPS.load(Relaxed), 2);
    }
}
