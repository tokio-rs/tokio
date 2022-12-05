#![cfg_attr(any(not(feature = "full"), tokio_wasm), allow(dead_code))]
use crate::runtime::Callback;
use crate::util::RngSeedGenerator;

pub(crate) struct Config {
    /// How many ticks before pulling a task from the global/remote queue?
    pub(crate) global_queue_interval: u32,

    /// How many ticks before yielding to the driver for timer and I/O events?
    pub(crate) event_interval: u32,

    /// Callback for a worker parking itself
    pub(crate) before_park: Option<Callback>,

    /// Callback for a worker unparking itself
    pub(crate) after_unpark: Option<Callback>,

    /// The multi-threaded scheduler includes a per-worker LIFO slot used to
    /// store the last scheduled task. This can improve certain usage patterns,
    /// especially message passing between tasks. However, this LIFO slot is not
    /// currently stealable.
    ///
    /// Eventually, the LIFO slot **will** become stealable, however as a
    /// stop-gap, this unstable option lets users disable the LIFO task.
    pub(crate) disable_lifo_slot: bool,

    /// Random number generator seed to configure runtimes to act in a
    /// deterministic way.
    pub(crate) seed_generator: RngSeedGenerator,

    #[cfg(tokio_unstable)]
    /// How to respond to unhandled task panics.
    pub(crate) unhandled_panic: crate::runtime::UnhandledPanic,
}
