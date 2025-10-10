// Currently, rust warns when an unsafe fn contains an unsafe {} block. However,
// in the future, this will change to the reverse. For now, suppress this
// warning and generally stick with being explicit about unsafety.
#![allow(unused_unsafe)]
#![cfg_attr(not(feature = "rt"), allow(dead_code))]

//! Time driver.

mod timer;
pub(crate) use timer::Timer;

mod handle;
pub(crate) use self::handle::Handle;

mod source;
pub(crate) use source::TimeSource;

mod wheel;
cfg_rt_and_time! {
    pub(crate) use wheel::{Insert, EntryHandle};
}
cfg_rt_or_time! {
    pub(crate) use wheel::cancellation_queue;
    pub(crate) use wheel::Wheel;
}

cfg_test_util! {
    use crate::loom::sync::Arc;
}

use crate::loom::sync::atomic::{AtomicBool, Ordering};
use crate::runtime::driver::{self, IoStack};
use crate::time::{Clock, Duration};

/// Time implementation that drives [`Sleep`][sleep], [`Interval`][interval], and [`Timeout`][timeout].
///
/// A `Driver` instance tracks the state necessary for managing time and
/// notifying the [`Sleep`][sleep] instances once their deadlines are reached.
///
/// It is expected that a single instance manages many individual [`Sleep`][sleep]
/// instances. The `Driver` implementation is thread-safe and, as such, is able
/// to handle callers from across threads.
///
/// After creating the `Driver` instance, the caller must repeatedly call `park`
/// or `park_timeout`. The time driver will perform no work unless `park` or
/// `park_timeout` is called repeatedly.
///
/// The driver has a resolution of one millisecond. Any unit of time that falls
/// between milliseconds are rounded up to the next millisecond.
///
/// When an instance is dropped, any outstanding [`Sleep`][sleep] instance that has not
/// elapsed will be notified with an error. At this point, calling `poll` on the
/// [`Sleep`][sleep] instance will result in panic.
///
/// # Implementation
///
/// The time driver is based on the [paper by Varghese and Lauck][paper].
///
/// A hashed timing wheel is a vector of slots, where each slot handles a time
/// slice. As time progresses, the timer walks over the slot for the current
/// instant, and processes each entry for that slot. When the timer reaches the
/// end of the wheel, it starts again at the beginning.
///
/// The implementation maintains six wheels arranged in a set of levels. As the
/// levels go up, the slots of the associated wheel represent larger intervals
/// of time. At each level, the wheel has 64 slots. Each slot covers a range of
/// time equal to the wheel at the lower level. At level zero, each slot
/// represents one millisecond of time.
///
/// The wheels are:
///
/// * Level 0: 64 x 1 millisecond slots.
/// * Level 1: 64 x 64 millisecond slots.
/// * Level 2: 64 x ~4 second slots.
/// * Level 3: 64 x ~4 minute slots.
/// * Level 4: 64 x ~4 hour slots.
/// * Level 5: 64 x ~12 day slots.
///
/// When the timer processes entries at level zero, it will notify all the
/// `Sleep` instances as their deadlines have been reached. For all higher
/// levels, all entries will be redistributed across the wheel at the next level
/// down. Eventually, as time progresses, entries with [`Sleep`][sleep] instances will
/// either be canceled (dropped) or their associated entries will reach level
/// zero and be notified.
///
/// [paper]: http://www.cs.columbia.edu/~nahum/w6998/papers/ton97-timing-wheels.pdf
/// [sleep]: crate::time::Sleep
/// [timeout]: crate::time::Timeout
/// [interval]: crate::time::Interval
#[derive(Debug)]
pub(crate) struct Driver {
    /// Parker to delegate to.
    park: IoStack,

    is_shutdown: AtomicBool,
}

// ===== impl Driver =====

impl Driver {
    /// Creates a new `Driver` instance that uses `park` to block the current
    /// thread and `time_source` to get the current time and convert to ticks.
    ///
    /// Specifying the source of time is useful when testing.
    pub(crate) fn new(park: IoStack, clock: &Clock) -> (Driver, Handle) {
        let time_source = TimeSource::new(clock);

        let handle = Handle {
            time_source,
            #[cfg(feature = "test-util")]
            did_wake: Arc::new(AtomicBool::new(false)),
        };

        let driver = Driver {
            park,
            is_shutdown: AtomicBool::new(false),
        };

        (driver, handle)
    }

    pub(crate) fn park(&mut self, handle: &driver::Handle) {
        self.park.park(handle);
    }

    pub(crate) fn park_timeout(&mut self, handle: &driver::Handle, duration: Duration) {
        self.park.park_timeout(handle, duration);
    }

    pub(crate) fn shutdown(&mut self, rt_handle: &driver::Handle) {
        if self.is_shutdown.load(Ordering::SeqCst) {
            return;
        }

        self.is_shutdown.store(true, Ordering::SeqCst);
        self.park.shutdown(rt_handle);
    }
}

cfg_rt_or_time! {
    /// Local context for the time driver, used when creating timers.
    pub(crate) enum Context<'a> {
        /// The runtime is running, we can access it.
        Running {
            /// the local time wheel
            wheel: &'a mut Wheel,
            /// channel to push timers that are pending cancellation
            canc_tx: &'a cancellation_queue::Sender,
        },
        #[cfg(feature = "rt-multi-thread")]
        /// The runtime is shutting down, no timers can be registered.
        Shutdown,
    }

    /// Local context for the time driver, used when the runtime wants to
    /// fire/cancel timers.
    pub(crate) struct Context2 {
        pub(crate) wheel: Wheel,
        pub(crate) canc_tx: cancellation_queue::Sender,
        pub(crate) canc_rx: cancellation_queue::Receiver,
    }

    impl Context2 {
        pub(crate) fn new() -> Self {
            let (canc_tx, canc_rx) = cancellation_queue::new();
            Self {
                wheel: Wheel::new(),
                canc_tx,
                canc_rx,
            }
        }
    }
}

#[cfg(test)]
mod tests;
