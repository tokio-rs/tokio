cfg_rt! {
    pub(crate) mod current_thread;
    pub(crate) use current_thread::CurrentThread;
}

cfg_rt_multi_thread! {
    pub(crate) mod multi_thread;
    pub(crate) use multi_thread::MultiThread;
}

use crate::runtime::driver;

#[derive(Debug, Clone)]
pub(crate) enum Handle {
    #[cfg(feature = "rt")]
    CurrentThread(Arc<current_thread::Handle>),

    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
    MultiThread(Arc<multi_thread::Handle>),

    // TODO: This is to avoid triggering "dead code" warnings many other places
    // in the codebase. Remove this during a later cleanup
    #[cfg(not(feature = "rt"))]
    #[allow(dead_code)]
    Disabled,
}

impl Handle {
    #[cfg_attr(not(feature = "full"), allow(dead_code))]
    pub(crate) fn driver(&self) -> &driver::Handle {
        match *self {
            #[cfg(feature = "rt")]
            Handle::CurrentThread(ref h) => &h.driver,

            #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
            Handle::MultiThread(ref h) => &h.driver,

            #[cfg(not(feature = "rt"))]
            Handle::Disabled => unreachable!(),
        }
    }

    cfg_time! {
        #[track_caller]
        pub(crate) fn time(&self) -> &crate::runtime::time::Handle {
            self.driver()
                .time
                .as_ref()
                .expect("A Tokio 1.x context was found, but timers are disabled. Call `enable_time` on the runtime builder to enable timers.")
        }

        cfg_test_util! {
            pub(crate) fn clock(&self) -> &driver::Clock {
                &self.driver().clock
            }
        }
    }
}

cfg_rt! {
    use crate::future::Future;
    use crate::loom::sync::Arc;
    use crate::runtime::{blocking, task::Id};
    use crate::task::JoinHandle;
    use crate::util::RngSeedGenerator;

    impl Handle {
        pub(crate) fn blocking_spawner(&self) -> &blocking::Spawner {
            match self {
                Handle::CurrentThread(h) => &h.blocking_spawner,

                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Handle::MultiThread(h) => &h.blocking_spawner,
            }
        }

        pub(crate) fn spawn<F>(&self, future: F, id: Id) -> JoinHandle<F::Output>
        where
            F: Future + Send + 'static,
            F::Output: Send + 'static,
        {
            match self {
                Handle::CurrentThread(h) => current_thread::Handle::spawn(h, future, id),

                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Handle::MultiThread(h) => multi_thread::Handle::spawn(h, future, id),
            }
        }

        pub(crate) fn shutdown(&self) {
            match *self {
                Handle::CurrentThread(_) => {},

                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Handle::MultiThread(ref h) => h.shutdown(),
            }
        }

        pub(crate) fn seed_generator(&self) -> &RngSeedGenerator {
            match self {
                Handle::CurrentThread(h) => &h.seed_generator,

                #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                Handle::MultiThread(h) => &h.seed_generator,
            }
        }

        #[cfg(unix)]
        cfg_signal_internal! {
            pub(crate) fn signal(&self) -> &driver::SignalHandle {
                &self.driver().signal
            }
        }
    }

    cfg_metrics! {
        use crate::runtime::{SchedulerMetrics, WorkerMetrics};

        impl Handle {
            pub(crate) fn num_workers(&self) -> usize {
                match self {
                    Handle::CurrentThread(_) => 1,
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                    Handle::MultiThread(handle) => handle.num_workers(),
                }
            }

            pub(crate) fn scheduler_metrics(&self) -> &SchedulerMetrics {
                match self {
                    Handle::CurrentThread(handle) => handle.scheduler_metrics(),
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                    Handle::MultiThread(handle) => handle.scheduler_metrics(),
                }
            }

            pub(crate) fn worker_metrics(&self, worker: usize) -> &WorkerMetrics {
                match self {
                    Handle::CurrentThread(handle) => handle.worker_metrics(worker),
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                    Handle::MultiThread(handle) => handle.worker_metrics(worker),
                }
            }

            pub(crate) fn injection_queue_depth(&self) -> usize {
                match self {
                    Handle::CurrentThread(handle) => handle.injection_queue_depth(),
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                    Handle::MultiThread(handle) => handle.injection_queue_depth(),
                }
            }

            pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
                match self {
                    Handle::CurrentThread(handle) => handle.worker_metrics(worker).queue_depth(),
                    #[cfg(all(feature = "rt-multi-thread", not(tokio_wasi)))]
                    Handle::MultiThread(handle) => handle.worker_local_queue_depth(worker),
                }
            }
        }
    }
}
