//! Abstracts out the APIs necessary to `Runtime` for integrating the time
//! driver. When the `time` feature flag is **not** enabled. These APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

pub(crate) use variant::*;

#[cfg(feature = "time")]
mod variant {
    use std::time::Duration;

    use crate::park::Either;
    use crate::runtime::io;
    use crate::time::{self, driver};

    pub(crate) type Clock = time::Clock;
    pub(crate) type Driver = Either<driver::Driver<io::Driver>, io::Driver>;
    pub(crate) type Handle = Option<driver::Handle>;

    pub(crate) fn create_clock() -> Clock {
        Clock::new()
    }

    /// Creates a new timer driver with optional throttling and its handle
    pub(crate) fn create_throttling_driver(
        enable: bool,
        io_driver: io::Driver,
        clock: Clock,
        max_throttling: Option<Duration>,
    ) -> (Driver, Handle) {
        if enable {
            let mut driver = driver::Driver::new(io_driver, clock);
            let handle = driver.handle();

            if max_throttling.is_some() {
                driver.enable_throttling();
            }

            (Either::A(driver), Some(handle))
        } else {
            (Either::B(io_driver), None)
        }
    }

    /// Creates a new timer driver / handle pair
    pub(crate) fn create_driver(
        enable: bool,
        io_driver: io::Driver,
        clock: Clock,
    ) -> (Driver, Handle) {
        create_throttling_driver(enable, io_driver, clock, None)
    }
}

#[cfg(not(feature = "time"))]
mod variant {
    #[cfg(feature = "rt-core")]
    use std::time::Duration;

    use crate::runtime::io;

    pub(crate) type Clock = ();
    pub(crate) type Driver = io::Driver;
    pub(crate) type Handle = ();

    pub(crate) fn create_clock() -> Clock {
        ()
    }

    /// Creates a new timer driver with optional throttling and its handle
    #[cfg(feature = "rt-core")]
    pub(crate) fn create_throttling_driver(
        _enable: bool,
        io_driver: io::Driver,
        _clock: Clock,
        _max_throttling: Option<Duration>,
    ) -> (Driver, Handle) {
        (io_driver, ())
    }

    /// Creates a new timer driver / handle pair
    pub(crate) fn create_driver(
        _enable: bool,
        io_driver: io::Driver,
        _clock: Clock,
    ) -> (Driver, Handle) {
        (io_driver, ())
    }
}
