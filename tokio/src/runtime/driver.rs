//! Abstracts out the entire chain of runtime sub-drivers into common types.
use crate::io::driver as io_driver;
use crate::park::{Either, Park, ParkThread};
use crate::time::{self, driver as time_driver};
use std::io;
use std::time::Duration;

// ===== io driver =====

cfg_io_driver! {
    type IoDriver = Either<io_driver::Driver, ParkThread>;
    pub(crate) type IoHandle = Option<io_driver::Handle>;

    fn create_io_driver(enable: bool) -> io::Result<(IoDriver, IoHandle)> {
        #[cfg(loom)]
        assert!(!enable);

        if enable {
            let driver = io_driver::Driver::new()?;
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

// ===== time driver =====

cfg_time! {
    type TimeDriver = Either<time_driver::Driver<IoDriver>, IoDriver>;

    pub(crate) type Clock = time::Clock;
    pub(crate) type TimeHandle = Option<time_driver::Handle>;

    fn create_clock() -> Clock {
        time::Clock::new()
    }

    fn create_time_driver(
        enable: bool,
        io_driver: IoDriver,
        clock: Clock,
    ) -> (TimeDriver, TimeHandle) {
        if enable {
            let driver = time_driver::Driver::new(io_driver, clock);
            let handle = driver.handle();

            (Either::A(driver), Some(handle))
        } else {
            (Either::B(io_driver), None)
        }
    }
}

cfg_not_time! {
    type TimeDriver = IoDriver;

    pub(crate) type Clock = ();
    pub(crate) type TimeHandle = ();

    fn create_clock() -> Clock {
        ()
    }

    fn create_time_driver(
        _enable: bool,
        io_driver: IoDriver,
        _clock: Clock,
    ) -> (TimeDriver, TimeHandle) {
        (io_driver, ())
    }
}

// ===== runtime driver =====

#[derive(Debug)]
pub(crate) struct Driver {
    inner: TimeDriver,
}

pub(crate) struct Resources {
    pub(crate) io_handle: IoHandle,
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
        let (driver, time_handle) = create_time_driver(cfg.enable_time, io_driver, clock.clone());

        Ok((
            Self { inner: driver },
            Resources {
                io_handle,
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
