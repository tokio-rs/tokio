//! Abstracts out the entire chain of runtime sub-drivers into common types.

// Eventually, this file will see significant refactoring / cleanup. For now, we
// don't need to worry much about dead code with certain feature permutations.
#![cfg_attr(not(feature = "full"), allow(dead_code))]

use crate::park::thread::{ParkThread, UnparkThread};

use std::io;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct Driver {
    inner: TimeDriver,
}

#[derive(Debug)]
pub(crate) struct Handle {
    /// IO driver handle
    pub(crate) io: IoHandle,

    /// Signal driver handle
    #[cfg_attr(any(not(unix), loom), allow(dead_code))]
    pub(crate) signal: SignalHandle,

    /// Time driver handle
    pub(crate) time: TimeHandle,

    /// Source of `Instant::now()`
    #[cfg_attr(not(all(feature = "time", feature = "test-util")), allow(dead_code))]
    pub(crate) clock: Clock,
}

pub(crate) struct Cfg {
    pub(crate) enable_io: bool,
    pub(crate) enable_time: bool,
    pub(crate) enable_pause_time: bool,
    pub(crate) start_paused: bool,
}

impl Driver {
    pub(crate) fn new(cfg: Cfg) -> io::Result<(Self, Handle)> {
        let (io_stack, io_handle, signal_handle) = create_io_stack(cfg.enable_io)?;

        let clock = create_clock(cfg.enable_pause_time, cfg.start_paused);

        let (time_driver, time_handle) =
            create_time_driver(cfg.enable_time, io_stack, clock.clone());

        Ok((
            Self { inner: time_driver },
            Handle {
                io: io_handle,
                signal: signal_handle,
                time: time_handle,
                clock,
            },
        ))
    }

    pub(crate) fn park(&mut self) {
        self.inner.park()
    }

    pub(crate) fn park_timeout(&mut self, duration: Duration) {
        self.inner.park_timeout(duration)
    }

    pub(crate) fn shutdown(&mut self) {
        self.inner.shutdown()
    }
}

impl Handle {
    pub(crate) fn unpark(&self) {
        #[cfg(feature = "time")]
        if let Some(handle) = &self.time {
            handle.unpark();
        }

        self.io.unpark();
    }
}

// ===== io driver =====

cfg_io_driver! {
    pub(crate) type IoDriver = crate::runtime::io::Driver;

    #[derive(Debug)]
    pub(crate) enum IoStack {
        Enabled(ProcessDriver),
        Disabled(ParkThread),
    }

    #[derive(Debug, Clone)]
    pub(crate) enum IoHandle {
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

            (IoStack::Enabled(process_driver), IoHandle::Enabled(io_handle), signal_handle)
        } else {
            let park_thread = ParkThread::new();
            let unpark_thread = park_thread.unpark();
            (IoStack::Disabled(park_thread), IoHandle::Disabled(unpark_thread), Default::default())
        };

        Ok(ret)
    }

    impl IoStack {
        /*
        pub(crate) fn handle(&self) -> IoHandle {
            match self {
                IoStack::Enabled(v) => IoHandle::Enabled(v.handle()),
                IoStack::Disabled(v) => IoHandle::Disabled(v.unpark()),
            }
        }]
        */

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

    impl IoHandle {
        pub(crate) fn unpark(&self) {
            match self {
                IoHandle::Enabled(handle) => handle.unpark(),
                IoHandle::Disabled(handle) => handle.unpark(),
            }
        }

        #[track_caller]
        pub(crate) fn expect(self, msg: &'static str) -> crate::runtime::io::Handle {
            match self {
                IoHandle::Enabled(v) => v,
                IoHandle::Disabled(..) => panic!("{}", msg),
            }
        }

        cfg_unstable! {
            pub(crate) fn as_ref(&self) -> Option<&crate::runtime::io::Handle> {
                match self {
                    IoHandle::Enabled(v) => Some(v),
                    IoHandle::Disabled(..) => None,
                }
            }
        }
    }
}

cfg_not_io_driver! {
    pub(crate) type IoHandle = UnparkThread;
    pub(crate) type IoStack = ParkThread;

    fn create_io_stack(_enabled: bool) -> io::Result<(IoStack, IoHandle, SignalHandle)> {
        let park_thread = ParkThread::new();
        let unpark_thread = park_thread.unpark();
        Ok((park_thread, unpark_thread, Default::default()))
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

        pub(crate) fn shutdown(&mut self) {
            match self {
                TimeDriver::Enabled { driver, handle } => driver.shutdown(handle),
                TimeDriver::Disabled(v) => v.shutdown(),
            }
        }
    }
}

cfg_not_time! {
    type TimeDriver = IoStack;

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
