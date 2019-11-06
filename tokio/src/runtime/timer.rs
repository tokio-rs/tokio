pub(crate) use self::variant::*;

#[cfg(feature = "timer")]
mod variant {
    use crate::runtime::io;
    use crate::timer::{clock, timer};

    pub(crate) type Clock = clock::Clock;
    pub(crate) type Driver = timer::Timer<io::Driver>;
    pub(crate) type Handle = timer::Handle;

    /// Create a new timer driver / handle pair
    pub(crate) fn create(io_driver: io::Driver, clock: Clock) -> (Driver, Handle) {
        let driver = timer::Timer::new_with_clock(io_driver, clock);
        let handle = driver.handle();

        (driver, handle)
    }

    #[cfg(feature = "blocking")]
    pub(crate) fn set_default(handle: &Handle) -> timer::DefaultGuard<'_> {
        timer::set_default(handle)
    }
}

#[cfg(not(feature = "timer"))]
mod variant {
    use crate::runtime::io;

    pub(crate) type Clock = ();
    pub(crate) type Driver = io::Driver;
    pub(crate) type Handle = ();

    /// Create a new timer driver / handle pair
    pub(crate) fn create(io_driver: io::Driver, _clock: Clock) -> (Driver, Handle) {
        (io_driver, ())
    }

    #[cfg(feature = "blocking")]
    pub(crate) fn set_default(_handle: &Handle) {}
}
