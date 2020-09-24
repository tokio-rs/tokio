//! Abstracts out the entire chain of runtime sub-drivers into common types.
use crate::park::{Park, ParkThread};
use std::io;
use std::time::Duration;

// ===== io driver =====

cfg_io_driver! {
    type IoDriver = crate::park::Either<crate::io::driver::Driver, crate::park::ParkThread>;
    pub(crate) type IoHandle = Option<crate::io::driver::Handle>;

    fn create_io_driver(enable: bool) -> io::Result<(IoDriver, IoHandle)> {
        use crate::park::Either;

        #[cfg(loom)]
        assert!(!enable);

        if enable {
            let driver = crate::io::driver::Driver::new()?;
            let handle = driver.handle();

            Ok((Either::A(driver), Some(handle)))
        } else {
            let driver = ParkThread::new();
            Ok((Either::B(driver), None))
        }
    }
}

cfg_not_io_driver! {
    type IoDriver = ParkThread;
    pub(crate) type IoHandle = ();

    fn create_io_driver(_enable: bool) -> io::Result<(IoDriver, IoHandle)> {
        let driver = ParkThread::new();
        Ok((driver, ()))
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
    type SignalDriver = crate::park::Either<crate::signal::unix::driver::Driver, IoDriver>;
    pub(crate) type SignalHandle = Option<crate::signal::unix::driver::Handle>;

    fn create_signal_driver(io_driver: IoDriver) -> io::Result<(SignalDriver, SignalHandle)> {
        use crate::park::Either;

        // Enable the signal driver if IO is also enabled
        match io_driver {
            Either::A(io_driver) => {
                let driver = crate::signal::unix::driver::Driver::new(io_driver)?;
                let handle = driver.handle();
                Ok((Either::A(driver), Some(handle)))
            }
            Either::B(_) => Ok((Either::B(io_driver), None)),
        }
    }
}

cfg_not_signal_internal! {
    type SignalDriver = IoDriver;
    pub(crate) type SignalHandle = ();

    fn create_signal_driver(io_driver: IoDriver) -> io::Result<(SignalDriver, SignalHandle)> {
        Ok((io_driver, ()))
    }
}

// ===== time driver =====

cfg_time! {
    type TimeDriver = crate::park::Either<crate::time::driver::Driver<SignalDriver>, SignalDriver>;

    pub(crate) type Clock = crate::time::Clock;
    pub(crate) type TimeHandle = Option<crate::time::driver::Handle>;

    fn create_clock() -> Clock {
        crate::time::Clock::new()
    }

    fn create_time_driver(
        enable: bool,
        signal_driver: SignalDriver,
        clock: Clock,
    ) -> (TimeDriver, TimeHandle) {
        use crate::park::Either;

        if enable {
            let driver = crate::time::driver::Driver::new(signal_driver, clock);
            let handle = driver.handle();

            (Either::A(driver), Some(handle))
        } else {
            (Either::B(signal_driver), None)
        }
    }
}

cfg_not_time! {
    type TimeDriver = SignalDriver;

    pub(crate) type Clock = ();
    pub(crate) type TimeHandle = ();

    fn create_clock() -> Clock {
        ()
    }

    fn create_time_driver(
        _enable: bool,
        signal_driver: SignalDriver,
        _clock: Clock,
    ) -> (TimeDriver, TimeHandle) {
        (signal_driver, ())
    }
}

// ===== runtime driver =====

#[derive(Debug)]
pub(crate) struct Driver {
    inner: TimeDriver,
}

pub(crate) struct Resources {
    pub(crate) io_handle: IoHandle,
    pub(crate) signal_handle: SignalHandle,
    pub(crate) time_handle: TimeHandle,
    pub(crate) clock: Clock,
}

pub(crate) struct Cfg {
    pub(crate) enable_io: bool,
    pub(crate) enable_time: bool,
}

impl Driver {
    pub(crate) fn new(cfg: Cfg) -> io::Result<(Self, Resources)> {
        let clock = create_clock();

        let (io_driver, io_handle) = create_io_driver(cfg.enable_io)?;
        let (signal_driver, signal_handle) = create_signal_driver(io_driver)?;
        let (time_driver, time_handle) =
            create_time_driver(cfg.enable_time, signal_driver, clock.clone());

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
}

impl Park for Driver {
    type Unpark = <TimeDriver as Park>::Unpark;
    type Error = <TimeDriver as Park>::Error;

    fn unpark(&self) -> Self::Unpark {
        self.inner.unpark()
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park()
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.inner.park_timeout(duration)
    }

    fn shutdown(&mut self) {
        self.inner.shutdown()
    }
}
