//! Default [Syscalls]
use super::Syscalls;
use std::future::Future;
use std::pin::Pin;

pub(crate) struct DefaultSyscalls;

impl Syscalls for DefaultSyscalls {
    fn spawn(&self, _future: Pin<Box<dyn Future<Output = ()>>>) {
        todo!("spawn")
    }

    fn spawn_blocking(&self, _task: Box<dyn FnOnce()>) {
        todo!("spawn_blocking")
    }

    fn park(&self) {
        todo!("park")
    }

    fn park_timeout(&self, _duration: std::time::Duration) {
        todo!("park_timeout")
    }

    fn unpark(&self) {
        todo!("unpark")
    }
}
