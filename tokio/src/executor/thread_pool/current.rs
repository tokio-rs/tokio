use crate::executor::loom::sync::Arc;
use crate::executor::park::Unpark;
use crate::executor::thread_pool::{worker, Owned};

use std::cell::Cell;
use std::ptr;

/// Tracks the current worker
#[derive(Debug)]
pub(super) struct Current {
    inner: Inner,
}

#[derive(Debug, Copy, Clone)]
struct Inner {
    // thread-local variables cannot track generics. However, the current worker
    // is only checked when `P` is already known, so the type can be figured out
    // on demand.
    workers: *const (),
    idx: usize,
}

// Pointer to the current worker info
thread_local!(static CURRENT_WORKER: Cell<Inner> = Cell::new(Inner::new()));

pub(super) fn set<F, R, P>(pool: &Arc<worker::Set<P>>, index: usize, f: F) -> R
where
    F: FnOnce() -> R,
    P: Unpark,
{
    CURRENT_WORKER.with(|cell| {
        assert!(cell.get().workers.is_null());

        struct Guard<'a>(&'a Cell<Inner>);

        impl Drop for Guard<'_> {
            fn drop(&mut self) {
                self.0.set(Inner::new());
            }
        }

        cell.set(Inner {
            workers: pool.shared() as *const _ as *const (),
            idx: index,
        });

        let _g = Guard(cell);

        f()
    })
}

pub(super) fn get<F, R>(f: F) -> R
where
    F: FnOnce(&Current) -> R,
{
    CURRENT_WORKER.with(|cell| {
        let current = Current { inner: cell.get() };
        f(&current)
    })
}

impl Current {
    pub(super) fn as_member<'a, P>(&self, set: &'a worker::Set<P>) -> Option<&'a Owned<P>>
    where
        P: Unpark,
    {
        let inner = CURRENT_WORKER.with(|cell| cell.get());

        if ptr::eq(inner.workers as *const _, set.shared().as_ptr()) {
            Some(unsafe { &*set.owned()[inner.idx].get() })
        } else {
            None
        }
    }
}

impl Inner {
    fn new() -> Inner {
        Inner {
            workers: ptr::null(),
            idx: 0,
        }
    }
}
