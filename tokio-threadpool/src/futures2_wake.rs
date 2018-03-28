use inner::Inner;
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

impl Futures2Wake {
    pub(crate) fn new(id: usize, inner: &Arc<Inner>) -> Futures2Wake {
        let notifier = Arc::new(Notifier {
            inner: Arc::downgrade(inner),
        });
        Futures2Wake { id, notifier }
    }
}

impl Drop for Futures2Wake {
    fn drop(&mut self) {
        self.notifier.drop_id(self.id)
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

pub(crate) fn into_waker(rc: Arc<Futures2Wake>) -> futures2::task::Waker {
    unsafe {
        let ptr = mem::transmute::<Arc<Futures2Wake>, *mut ArcWrapped>(rc);
        futures2::task::Waker::new(ptr)
    }
}
