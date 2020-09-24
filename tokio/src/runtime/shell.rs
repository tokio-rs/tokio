#![allow(clippy::redundant_clone)]

use crate::future::poll_fn;
use crate::park::{Park, Unpark};
use crate::runtime::driver::Driver;
use crate::sync::Notify;
use crate::util::{waker_ref, Wake};

use std::sync::{Arc, Mutex};
use std::task::Context;
use std::task::Poll::{Pending, Ready};
use std::{future::Future, sync::PoisonError};

#[derive(Debug)]
pub(super) struct Shell {
    driver: Mutex<Option<Driver>>,

    notify: Notify,

    /// TODO: don't store this
    unpark: Arc<Handle>,
}

#[derive(Debug)]
struct Handle(<Driver as Park>::Unpark);

impl Shell {
    pub(super) fn new(driver: Driver) -> Shell {
        let unpark = Arc::new(Handle(driver.unpark()));

        Shell {
            driver: Mutex::new(Some(driver)),
            notify: Notify::new(),
            unpark,
        }
    }

    pub(super) fn block_on<F>(&self, f: F) -> F::Output
    where
        F: Future,
    {
        let mut enter = crate::runtime::enter(true);

        pin!(f);

        loop {
            if let Some(driver) = &mut self.take_driver() {
                return driver.block_on(f);
            } else {
                let notified = self.notify.notified();
                pin!(notified);

                if let Some(out) = enter
                    .block_on(poll_fn(|cx| {
                        if notified.as_mut().poll(cx).is_ready() {
                            return Ready(None);
                        }

                        if let Ready(out) = f.as_mut().poll(cx) {
                            return Ready(Some(out));
                        }

                        Pending
                    }))
                    .expect("Failed to `Enter::block_on`")
                {
                    return out;
                }
            }
        }
    }

    fn take_driver(&self) -> Option<DriverGuard<'_>> {
        let mut lock = self.driver.lock().unwrap();
        let driver = lock.take()?;

        Some(DriverGuard {
            inner: Some(driver),
            shell: &self,
        })
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

struct DriverGuard<'a> {
    inner: Option<Driver>,
    shell: &'a Shell,
}

impl DriverGuard<'_> {
    fn block_on<F: Future>(&mut self, f: F) -> F::Output {
        let driver = self.inner.as_mut().unwrap();

        pin!(f);

        let waker = waker_ref(&self.shell.unpark);
        let mut cx = Context::from_waker(&waker);

        loop {
            if let Ready(v) = crate::coop::budget(|| f.as_mut().poll(&mut cx)) {
                return v;
            }

            driver.park().unwrap();
        }
    }
}

impl Drop for DriverGuard<'_> {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.take() {
            self.shell
                .driver
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .replace(inner);

            self.shell.notify.notify_one();
        }
    }
}
