use super::{cancellation_queue, Wheel};

/// Local context for the time driver, used when the runtime wants to
/// fire/cancel timers.
pub(crate) struct LocalContext {
    pub(crate) wheel: Wheel,
    pub(crate) canc_tx: cancellation_queue::Sender,
    pub(crate) canc_rx: cancellation_queue::Receiver,
}

impl LocalContext {
    pub(crate) fn new() -> Self {
        let (canc_tx, canc_rx) = cancellation_queue::new();
        Self {
            wheel: Wheel::new(),
            canc_tx,
            canc_rx,
        }
    }
}

pub(crate) enum TempLocalContext<'a> {
    /// The runtime is running, we can access it.
    Running {
        wheel: &'a mut Wheel,
        canc_tx: &'a cancellation_queue::Sender,
    },
    /// The runtime is shutting down, no timers can be registered.
    Shutdown,
}

impl<'a> TempLocalContext<'a> {
    pub(crate) fn new_running(cx: &'a mut LocalContext) -> Self {
        Self::Running {
            wheel: &mut cx.wheel,
            canc_tx: &cx.canc_tx,
        }
    }

    pub(crate) fn new_shutdown() -> Self {
        Self::Shutdown
    }
}
