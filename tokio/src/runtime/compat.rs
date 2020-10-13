use std::sync::{Arc, Weak};
use crate::task::JoinHandle;
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use parking_lot::RwLock;
use std::time::Duration;
use crate::runtime::task;

#[derive(Debug)]
pub(crate) struct Compat03Runtime {
    shared: Arc<Shared>,
    wait_on_drop: bool,
    // This keeps the IO driver alive
    rt02_chan: crate::sync::oneshot::Sender<()>,
}

#[derive(Debug, Clone)]
pub(crate) struct Compat03Handle {
    shared: Weak<Shared>,
}

#[derive(Debug)]
struct Shared {
    runtime: RwLock<Option<tokio_03::runtime::Runtime>>,
    num_read_locks: AtomicUsize,
    kill_on_unlock: AtomicBool,
}

impl Compat03Runtime {
    pub(crate) fn new(
        rt: tokio_03::runtime::Runtime,
        driver: crate::runtime::Runtime
    ) -> Self {
        let (rt02_chan, rt02_recv) = crate::sync::oneshot::channel();

        std::thread::spawn(move || {
            let _ = driver.block_on(rt02_recv);
        });

        Self {
            shared: Arc::new(Shared {
                runtime: RwLock::new(Some(rt)),
                num_read_locks: AtomicUsize::new(0),
                kill_on_unlock: AtomicBool::new(false),
            }),
            wait_on_drop: true,
            rt02_chan,
        }
    }
    pub(crate) fn handle(&self) -> Compat03Handle {
        Compat03Handle {
            shared: Arc::downgrade(&self.shared),
        }
    }
    pub(crate) fn take(self, timeout: Duration) -> Option<tokio_03::runtime::Runtime> {
        self.wait_on_drop = false;
        self.shared.take_timeout_or_kill(timeout)
    }

    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match self.shared.spawn(future) {
            Some(shared) => shared,
            None => task::joinable(async move { unreachable!() }).1,
        }
    }

    pub(crate) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.shared.block_on(future).expect("Runtime shut down")
    }
}

impl Compat03Handle {
    pub(crate) fn spawn<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        match self.shared.upgrade().and_then(move |shared| shared.spawn(future)) {
            Some(shared) => shared,
            None => task::joinable(async move { unreachable!() }).1,
        }
    }

    pub(crate) fn block_on<F>(&self, future: F) -> F::Output
    where
        F: Future,
    {
        self.shared.upgrade()
            .and_then(|shared| shared.block_on(future))
            .expect("Runtime shut down")
    }
}

impl Shared {
    fn spawn<F>(&self, future: F) -> Option<JoinHandle<F::Output>>
    where
        F: Future + Send + 'static,
        F::Output: Send + 'static,
    {
        self.with_runtime(move |runtime| {
            JoinHandle::new_compat(runtime.spawn(future))
        })
    }

    fn block_on<F>(&self, future: F) -> Option<F::Output>
    where
        F: Future,
    {
        self.with_runtime(move |runtime| {
            runtime.block_on(future)
        })
    }

    fn with_runtime<F, O>(&self, func: F) -> Option<O>
    where
        F: FnOnce(&tokio_03::runtime::Runtime) -> O
    {
        if self.kill_on_unlock.load(Ordering::SeqCst) {
            return None;
        }
        self.num_read_locks.fetch_add(1, Ordering::SeqCst);

        struct Guard<'a> {
            shared: &'a Shared,
        }
        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                let num_locks = self.shared.num_read_locks.fetch_sub(1, Ordering::SeqCst);
                let is_last = num_locks == 1;
                let kill_on_unlock = self.shared.kill_on_unlock.load(Ordering::SeqCst);
                if kill_on_unlock && is_last {
                    if let Some(rt) = self.shared.runtime.write().take() {
                        rt.shutdown_background();
                    }
                }
            }
        }

        let guard = Guard { shared: self };
        let rw_guard = self.runtime.read();
        let res = rw_guard.as_ref().map(func);
        drop(rw_guard);
        drop(guard);

        res
    }

    // Take out the runtime from the rwlock, waiting at most `dur` for active read locks
    // to give up the lock. If the lock is not obtained in this duration, ask the last
    // read lock to shut down the runtime when it finishes.
    fn take_timeout_or_kill(&self, dur: Duration) -> Option<tokio_03::runtime::Runtime> {
        match self.runtime.try_write_for(dur) {
            Some(runtime_opt) => runtime_opt.take(),
            None => {
                // Ask the last read lock to kill the runtime when it is done.
                self.kill_on_unlock.store(true, Ordering::SeqCst);

                // In case the last read lock exited before we got to make the atomic
                // write, try to lock it again.
                match self.runtime.try_write() {
                    Some(runtime_opt) => runtime_opt.take(),
                    None => None,
                }
            },
        }
    }
}

impl Drop for Compat03Runtime {
    fn drop(&mut self) {
        if self.wait_on_drop {
            // This will wait indefinitely for the lock.
            drop(self.shared.runtime.write().take());
        }
    }
}
