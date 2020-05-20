use crate::park::{Park, Unpark};
use crate::runtime::enter;
use crate::syscall::Syscalls;
use crate::task::JoinHandle;
use crate::util::{waker_ref, Wake};
use std::future::Future;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll::Ready;

#[derive(Clone)]
pub(crate) struct SyscallExecutor {
    inner: Arc<dyn Syscalls>,
}

impl std::fmt::Debug for SyscallExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SyscallRuntime").finish()
    }
}

#[allow(dead_code)]
impl SyscallExecutor {
    pub(super) fn new(inner: Arc<dyn Syscalls>) -> SyscallExecutor {
        SyscallExecutor { inner }
    }

    pub(super) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        let (wrapped, handle) = JoinHandle::<F::Output>::wrap(future);
        self.inner.spawn(Box::pin(wrapped));
        return handle;
    }

    pub(super) fn block_on<F>(&mut self, f: F) -> F::Output
    where
        F: Future,
    {
        let _e = enter(true);
        pin!(f);

        let mut park = SyscallPark {
            inner: Arc::clone(&self.inner),
        };

        // TODO: Get rid of double Arc
        let unpark = Arc::new(park.unpark());
        let waker = waker_ref(&unpark);
        let mut cx = Context::from_waker(&waker);
        loop {
            if let Ready(v) = crate::coop::budget(|| f.as_mut().poll(&mut cx)) {
                return v;
            }
            park.park().unwrap();
        }
    }
}

struct SyscallPark {
    inner: Arc<dyn Syscalls>,
}

impl Park for SyscallPark {
    type Unpark = SyscallUnpark;
    type Error = std::io::Error;

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park()
    }

    fn park_timeout(&mut self, duration: std::time::Duration) -> Result<(), Self::Error> {
        self.inner.park_timeout(duration)
    }

    fn unpark(&self) -> Self::Unpark {
        let inner = Arc::clone(&self.inner);
        SyscallUnpark { inner }
    }
}

struct SyscallUnpark {
    inner: Arc<dyn Syscalls>,
}

impl Unpark for SyscallUnpark {
    fn unpark(&self) {
        self.inner.unpark()
    }
}

impl Wake for SyscallUnpark {
    fn wake(self: Arc<Self>) {
        Wake::wake_by_ref(&self);
    }

    fn wake_by_ref(arc_self: &Arc<Self>) {
        arc_self.inner.unpark();
    }
}
