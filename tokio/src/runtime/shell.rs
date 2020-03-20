#![allow(clippy::redundant_clone)]

use crate::park::{Park, Unpark};
use crate::runtime::enter;
use crate::runtime::time;
use crate::util::{waker_ref, Wake};

use std::future::Future;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll::Ready;

#[derive(Debug)]
pub(super) struct Shell {
    driver: time::Driver,

    /// TODO: don't store this
    unpark: Arc<Handle>,
}

#[derive(Debug)]
struct Handle(<time::Driver as Park>::Unpark);

impl Shell {
    pub(super) fn new(driver: time::Driver) -> Shell {
        let unpark = Arc::new(Handle(driver.unpark()));

        Shell { driver, unpark }
    }

    pub(super) fn block_on<F>(&mut self, f: F) -> F::Output
    where
        F: Future,
    {
        let _e = enter();

        pin!(f);

        let waker = waker_ref(&self.unpark);
        let mut cx = Context::from_waker(&waker);

        loop {
            if let Ready(v) = crate::coop::budget(|| f.as_mut().poll(&mut cx)) {
                return v;
            }

            self.driver.park().unwrap();
        }
    }
}

impl Wake for Handle {
    /// Wake by value
    fn wake(self: Arc<Self>) {
        Wake::wake_by_ref(&self);
    }

    /// Wake by reference
    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.0.unpark();
    }
}
