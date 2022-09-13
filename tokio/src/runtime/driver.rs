//! Abstracts out the entire chain of runtime sub-drivers into common types.
use crate::park::thread::{ParkThread, UnparkThread};

use std::io;
use std::time::Duration;

// ===== io driver =====

cfg_io_driver! {
    pub(crate) type IoDriver = crate::runtime::io::Driver;
    pub(crate) type IoHandle = Option<crate::runtime::io::Handle>;

    #[derive(Debug)]
    pub(crate) enum IoStack {
        Enabled(ProcessDriver),
        Disabled(ParkThread),
    }

    pub(crate) enum IoUnpark {
        Enabled(crate::runtime::io::Handle),
        Disabled(UnparkThread),
    }

    fn create_io_stack(enabled: bool) -> io::Result<(IoStack, IoHandle, SignalHandle)> {
        #[cfg(loom)]
        assert!(!enabled);

        let ret = if enabled {
            let io_driver = crate::runtime::io::Driver::new()?;
            let io_handle = io_driver.handle();

            let (signal_driver, signal_handle) = create_signal_driver(io_driver)?;
            let process_driver = create_process_driver(signal_driver);

            (IoStack::Enabled(process_driver), Some(io_handle), signal_handle)
        } else {
            (IoStack::Disabled(ParkThread::new()), Default::default(), Default::default())
        };

        Ok(ret)
    }

    impl IoStack {
        pub(crate) fn unpark(&self) -> IoUnpark {
            match self {
                IoStack::Enabled(v) => IoUnpark::Enabled(v.handle()),
                IoStack::Disabled(v) => IoUnpark::Disabled(v.unpark()),
            }
        }

        pub(crate) fn park(&mut self) {
            match self {
                IoStack::Enabled(v) => v.park(),
                IoStack::Disabled(v) => v.park(),
            }
        }

        pub(crate) fn park_timeout(&mut self, duration: Duration) {
            match self {
                IoStack::Enabled(v) => v.park_timeout(duration),
                IoStack::Disabled(v) => v.park_timeout(duration),
            }
        }

        pub(crate) fn shutdown(&mut self) {
            match self {
                IoStack::Enabled(v) => v.shutdown(),
                IoStack::Disabled(v) => v.shutdown(),
            }
        }
    }

    impl IoUnpark {
        pub(crate) fn unpark(&self) {
            match self {
                IoUnpark::Enabled(v) => v.unpark(),
                IoUnpark::Disabled(v) => v.unpark(),
            }
        }
    }
}

cfg_not_io_driver! {
    pub(crate) type IoHandle = ();
    pub(crate) type IoStack = ParkThread;
    pub(crate) type IoUnpark = UnparkThread;

    fn create_io_stack(_enabled: bool) -> io::Result<(IoStack, IoHandle, SignalHandle)> {
        Ok((ParkThread::new(), Default::default(), Default::default()))
    }
}

// ===== signal driver =====

macro_rules! cfg_signal_internal_and_unix {
    ($($item:item)*) => {
        #[cfg(unix)]
        cfg_signal_internal! { $($item)* }
    }
}

cfg_signal_internal_and_unix! {
    type SignalDriver = crate::signal::unix::driver::Driver;
    pub(crate) type SignalHandle = Option<crate::signal::unix::driver::Handle>;

    fn create_signal_driver(io_driver: IoDriver) -> io::Result<(SignalDriver, SignalHandle)> {
        let driver = crate::signal::unix::driver::Driver::new(io_driver)?;
        let handle = driver.handle();
        Ok((driver, Some(handle)))
    }
}

cfg_not_signal_internal! {
    pub(crate) type SignalHandle = ();

    cfg_io_driver! {
        type SignalDriver = IoDriver;

        fn create_signal_driver(io_driver: IoDriver) -> io::Result<(SignalDriver, SignalHandle)> {
            Ok((io_driver, ()))
        }
    }
}

// ===== process driver =====

cfg_process_driver! {
    type ProcessDriver = crate::process::unix::driver::Driver;

    fn create_process_driver(signal_driver: SignalDriver) -> ProcessDriver {
        crate::process::unix::driver::Driver::new(signal_driver)
    }
}

cfg_not_process_driver! {
    cfg_io_driver! {
        type ProcessDriver = SignalDriver;

        fn create_process_driver(signal_driver: SignalDriver) -> ProcessDriver {
            signal_driver
        }
    }
}

// ===== time driver =====

cfg_time! {
    #[derive(Debug)]
    pub(crate) enum TimeDriver {
        Enabled {
            driver: crate::runtime::time::Driver,
            handle: crate::runtime::time::Handle,
        },
        Disabled(IoStack),
    }

    pub(crate) enum TimerUnpark {
        Enabled(crate::runtime::time::TimerUnpark),
        Disabled(IoUnpark),
    }

    pub(crate) type Clock = crate::time::Clock;
    pub(crate) type TimeHandle = Option<crate::runtime::time::Handle>;

    fn create_clock(enable_pausing: bool, start_paused: bool) -> Clock {
        crate::time::Clock::new(enable_pausing, start_paused)
    }

    fn create_time_driver(
        enable: bool,
        io_stack: IoStack,
        clock: Clock,
    ) -> (TimeDriver, TimeHandle) {
        if enable {
            let (driver, handle) = crate::runtime::time::Driver::new(io_stack, clock);

            (TimeDriver::Enabled { driver, handle: handle.clone() }, Some(handle))
        } else {
            (TimeDriver::Disabled(io_stack), None)
        }
    }

    impl TimeDriver {
        pub(crate) fn unpark(&self) -> TimerUnpark {
            match self {
                TimeDriver::Enabled { driver, .. } => TimerUnpark::Enabled(driver.unpark()),
                TimeDriver::Disabled(v) => TimerUnpark::Disabled(v.unpark()),
            }
        }

        pub(crate) fn park(&mut self) {
            match self {
                TimeDriver::Enabled { driver, handle } => driver.park(handle),
                TimeDriver::Disabled(v) => v.park(),
            }
        }

        pub(crate) fn park_timeout(&mut self, duration: Duration) {
            match self {
                TimeDriver::Enabled { driver, handle } => driver.park_timeout(handle, duration),
                TimeDriver::Disabled(v) => v.park_timeout(duration),
            }
        }

        // TODO: tokio-rs/tokio#4990, should the `current_thread` scheduler call this?
        cfg_rt_multi_thread! {
            pub(crate) fn shutdown(&mut self) {
                match self {
                    TimeDriver::Enabled { driver, handle } => driver.shutdown(handle),
                    TimeDriver::Disabled(v) => v.shutdown(),
                }
            }
        }
    }

    impl Drop for TimeDriver {
        fn drop(&mut self) {
            if let TimeDriver::Enabled { driver, handle } = self {
                driver.shutdown(handle);
            }
        }
    }

    impl TimerUnpark {
        pub(crate) fn unpark(&self) {
            match self {
                TimerUnpark::Enabled(v) => v.unpark(),
                TimerUnpark::Disabled(v) => v.unpark(),
            }
        }
    }
}

cfg_not_time! {
    type TimeDriver = IoStack;
    type TimerUnpark = IoUnpark;

    pub(crate) type Clock = ();
    pub(crate) type TimeHandle = ();

    fn create_clock(_enable_pausing: bool, _start_paused: bool) -> Clock {
        ()
    }

    fn create_time_driver(
        _enable: bool,
        io_stack: IoStack,
        _clock: Clock,
    ) -> (TimeDriver, TimeHandle) {
        (io_stack, ())
    }
}

// ===== runtime driver =====

#[derive(Debug)]
pub(crate) struct Driver {
    inner: TimeDriver,
}

pub(crate) type Unpark = TimerUnpark;

pub(crate) struct Resources {
    pub(crate) io_handle: IoHandle,
    pub(crate) signal_handle: SignalHandle,
    pub(crate) time_handle: TimeHandle,
    pub(crate) clock: Clock,
}

pub(crate) struct Cfg {
    pub(crate) enable_io: bool,
    pub(crate) enable_time: bool,
    pub(crate) enable_pause_time: bool,
    pub(crate) start_paused: bool,
}

impl Driver {
    pub(crate) fn new(cfg: Cfg) -> io::Result<(Self, Resources)> {
        let (io_stack, io_handle, signal_handle) = create_io_stack(cfg.enable_io)?;

        let clock = create_clock(cfg.enable_pause_time, cfg.start_paused);

        let (time_driver, time_handle) =
            create_time_driver(cfg.enable_time, io_stack, clock.clone());

        Ok((
            Self { inner: time_driver },
            Resources {
                io_handle,
                signal_handle,
                time_handle,
                clock,
            },
        ))
    }

    pub(crate) fn unpark(&self) -> TimerUnpark {
        self.inner.unpark()
    }

    pub(crate) fn park(&mut self) {
        self.inner.park()
    }

    pub(crate) fn park_timeout(&mut self, duration: Duration) {
        self.inner.park_timeout(duration)
    }

    // TODO: tokio-rs/tokio#4990, should the `current_thread` scheduler call this?
    cfg_rt_multi_thread! {
        pub(crate) fn shutdown(&mut self) {
            self.inner.shutdown()
        }
    }
}
