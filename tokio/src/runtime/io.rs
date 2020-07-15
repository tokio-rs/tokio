//! Abstracts out the APIs necessary to `Runtime` for integrating the I/O
//! driver. When the `time` feature flag is **not** enabled. These APIs are
//! shells. This isolates the complexity of dealing with conditional
//! compilation.

/// Re-exported for convenience.
pub(crate) use std::io::Result;

pub(crate) use variant::*;

#[cfg(feature = "io-driver")]
mod variant {
    use crate::io::driver;
    use crate::park::{Either, ParkThread};

    use std::io;

    /// The driver value the runtime passes to the `timer` layer.
    ///
    /// When the `io-driver` feature is enabled, this is the "real" I/O driver
    /// backed by Mio. Without the `io-driver` feature, this is a thread parker
    /// backed by a condition variable.
    pub(crate) type Driver = Either<driver::Driver, ParkThread>;

    /// The handle the runtime stores for future use.
    ///
    /// When the `io-driver` feature is **not** enabled, this is `()`.
    pub(crate) type Handle = Option<driver::Handle>;

    pub(crate) fn create_driver(enable: bool) -> io::Result<(Driver, Handle)> {
        #[cfg(loom)]
        assert!(!enable);

        if enable {
            let driver = driver::Driver::new()?;
            let handle = driver.handle();

            Ok((Either::A(driver), Some(handle)))
        } else {
            let driver = ParkThread::new();
            Ok((Either::B(driver), None))
        }
    }
}

#[cfg(not(feature = "io-driver"))]
mod variant {
    use crate::park::ParkThread;

    use std::io;

    /// I/O is not enabled, use a condition variable based parker
    pub(crate) type Driver = ParkThread;

    /// There is no handle
    pub(crate) type Handle = ();

    pub(crate) fn create_driver(_enable: bool) -> io::Result<(Driver, Handle)> {
        let driver = ParkThread::new();

        Ok((driver, ()))
    }
}
