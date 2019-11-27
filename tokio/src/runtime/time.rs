//! Abstracts out the APIs necessary to `Runtime` for integrating the time
//! driver. When the `time` feature flag is **not** enabled. These APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

pub(crate) use variant::*;

#[cfg(all(feature = "time", not(loom)))]
mod variant {
    use crate::park::Either;
    use crate::runtime::io;
    use crate::time::{self, driver};

    pub(crate) type Clock = time::Clock;
    pub(crate) type Driver = Either<driver::Driver<io::Driver>, io::Driver>;
    pub(crate) type Handle = Option<driver::Handle>;

    pub(crate) fn create_clock() -> Clock {
        Clock::new()
    }

    /// Create a new timer driver / handle pair
    pub(crate) fn create_driver(
        enable: bool,
        io_driver: io::Driver,
        clock: Clock,
    ) -> (Driver, Handle) {
        if enable {
            let driver = driver::Driver::new(io_driver, clock);
            let handle = driver.handle();

            (Either::A(driver), Some(handle))
        } else {
            (Either::B(io_driver), None)
        }
    }

    pub(crate) fn with_default<F, R>(handle: &Handle, clock: &Clock, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let _time = handle.as_ref().map(|handle| driver::set_default(handle));
        clock.enter(f)
    }
}

#[cfg(any(not(feature = "time"), loom))]
mod variant {
    use crate::runtime::io;

    pub(crate) type Clock = ();
    pub(crate) type Driver = io::Driver;
    pub(crate) type Handle = ();

    pub(crate) fn create_clock() -> Clock {
        ()
    }

    /// Create a new timer driver / handle pair
    pub(crate) fn create_driver(
        _enable: bool,
        io_driver: io::Driver,
        _clock: Clock,
    ) -> (Driver, Handle) {
        (io_driver, ())
    }

    pub(crate) fn with_default<F, R>(_handler: &Handle, _clock: &Clock, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        f()
    }
}
