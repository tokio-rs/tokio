use pool::Pool;
use notifier::Notifier;

use std::marker::PhantomData;
use std::mem;
use std::sync::Arc;

use futures::executor::Notify;
use futures2;

pub(crate) struct Futures2Wake {
    notifier: Arc<Notifier>,
    id: usize,
}

// Futures2Wake doesn't need to drop_id on drop,
// because the Futures2Wake is **only** ever held in:
// - ::task::Task, which handles drop itself
// - futures::Waker, that a user could have cloned. When that drops,
//   it will call drop_raw, so we don't need to double drop.
impl Futures2Wake {
    pub(crate) fn new(id: usize, inner: &Arc<Pool>) -> Futures2Wake {
        let notifier = Arc::new(Notifier {
            inner: Arc::downgrade(inner),
        });
        Futures2Wake { id, notifier }
    }
}

struct ArcWrapped(PhantomData<Futures2Wake>);

unsafe impl futures2::task::UnsafeWake for ArcWrapped {
    unsafe fn clone_raw(&self) -> futures2::task::Waker {
        let me: *const ArcWrapped = self;
        let arc = (*(&me as *const *const ArcWrapped as *const Arc<Futures2Wake>)).clone();
        arc.notifier.clone_id(arc.id);
        into_waker(arc)
    }

    unsafe fn drop_raw(&self) {
        let mut me: *const ArcWrapped = self;
        let me = &mut me as *mut *const ArcWrapped as *mut Arc<Futures2Wake>;
        (*me).notifier.drop_id((*me).id);
        ::std::ptr::drop_in_place(me);
    }

    unsafe fn wake(&self) {
        let me: *const ArcWrapped = self;
        let me = &me as *const *const ArcWrapped as *const Arc<Futures2Wake>;
        (*me).notifier.notify((*me).id)
    }
}

pub(crate) fn into_unsafe_wake(rc: Arc<Futures2Wake>) -> *mut futures2::task::UnsafeWake {
    unsafe {
        mem::transmute::<Arc<Futures2Wake>, *mut ArcWrapped>(rc)
    }
}

pub(crate) fn into_waker(rc: Arc<Futures2Wake>) -> futures2::task::Waker {
    unsafe {
        futures2::task::Waker::new(into_unsafe_wake(rc))
    }
}


#[cfg(test)]
mod tests {
    // We want most tests as integration tests, but these ones are special:
    //
    // This is testing that Task drop never happens more than it should,
    // causing use-after-free bugs. ;_;

    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::{Relaxed, Release};
    use ::{Sender, Shutdown, ThreadPool};

    use futures2;
    use futures2::prelude::*;

    static TASK_DROPS: AtomicUsize = ::std::sync::atomic::ATOMIC_USIZE_INIT;

    pub(super) fn on_task_drop() {
        TASK_DROPS.fetch_add(1, Release);
    }

    fn reset_task_drops() {
        TASK_DROPS.store(0, Release);
    }

    fn spawn_pool<F>(pool: &mut Sender, f: F)
        where F: Future<Item = (), Error = ()> + Send + 'static
    {
        futures2::executor::Executor::spawn(
            pool,
            Box::new(f.map_err(|_| panic!()))
        ).unwrap()
    }

    fn await_shutdown(shutdown: Shutdown) {
        ::futures::Future::wait(shutdown).unwrap()
    }

    #[test]
    fn task_drop_counts() {
        extern crate env_logger;
        let _ = env_logger::init();

        struct Always;

        impl Future for Always {
            type Item = ();
            type Error = ();

            fn poll(&mut self, _: &mut futures2::task::Context) -> Poll<(), ()> {
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

            fn poll(&mut self, cx: &mut futures2::task::Context) -> Poll<(), ()> {
                cx.waker().clone().wake();
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
