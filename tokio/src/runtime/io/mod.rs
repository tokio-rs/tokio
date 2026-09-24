#![cfg_attr(
    not(all(feature = "rt", feature = "net", feature = "io-uring", tokio_unstable)),
    allow(dead_code)
)]
mod driver;
#[cfg(all(test, loom))]
pub(crate) use driver::dispatch_event;
use driver::Tick;
pub(crate) use driver::{Direction, Driver, Handle, ReadyEvent};

mod registration;
pub(crate) use registration::Registration;

mod registration_set;
pub(crate) use registration_set::RegistrationSet;
#[cfg(all(test, loom))]
pub(crate) use registration_set::Synced;

mod scheduled_io;
pub(crate) use scheduled_io::ScheduledIo;

mod metrics;
use metrics::IoDriverMetrics;

use crate::util::ptr_expose::PtrExposeDomain;
static EXPOSE_IO: PtrExposeDomain<ScheduledIo> = PtrExposeDomain::new();
