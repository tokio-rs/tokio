//! The thread that parks in the driver on the runtime's behalf.
//!
//! It runs the same `park` a native runtime's thread would, so I/O
//! readiness, timer deadlines and signal delivery all happen here, and reach
//! the scheduler through the cross-thread schedule, which wakes the host.
//! A nearer timer registered from a drive unparks it to re-arm, as on a
//! multi-thread runtime.

use super::Shared;
use crate::runtime::driver::Driver;
use crate::runtime::Handle;

use std::io;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug)]
pub(super) struct Reactor {
    thread: std::thread::JoinHandle<Driver>,
    stop: Arc<AtomicBool>,
}

impl Reactor {
    pub(super) fn start(shared: &Rc<Shared>) -> io::Result<Reactor> {
        let scheduler = shared.handle.inner.as_current_thread();
        let mut driver = shared
            .runtime
            .current_thread()
            .take_driver(scheduler)
            .expect("driver missing");
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let handle = shared.handle.clone();
            let stop = stop.clone();
            std::thread::Builder::new()
                .name("tokio-event-loop-driver".into())
                .spawn(move || {
                    while !stop.load(Ordering::Acquire) {
                        driver.park(handle.inner.driver());
                    }
                    driver
                })?
        };
        Ok(Reactor { thread, stop })
    }

    pub(super) fn attach(&self) -> io::Result<()> {
        Ok(())
    }

    pub(super) fn after_turn(&self, _handle: &Handle) {}

    pub(super) fn stop(self, handle: &Handle) -> Option<Driver> {
        self.stop.store(true, Ordering::Release);
        handle.inner.driver().unpark();
        self.thread.join().ok()
    }
}
