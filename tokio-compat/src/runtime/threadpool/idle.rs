use std::future::Future;
use std::sync::{
    atomic::{fence, AtomicUsize, Ordering},
    Arc,
};
use tokio_02::sync::mpsc;

/// Tracks the number of tasks spawned on a runtime.
///
/// This is required to implement `shutdown_on_idle` and `tokio::run` APIs that
/// exist in `tokio` 0.1, as the `tokio` 0.2 threadpool does not expose a
/// `shutdown_on_idle` API.
#[derive(Clone, Debug)]
pub(super) struct Idle {
    tx: mpsc::Sender<()>,
    spawned: Arc<AtomicUsize>,
}

/// Wraps a future to decrement the spawned count when it completes.
///
/// This is obtained from `Idle::reserve`.
pub(super) struct Track(Idle);

impl Idle {
    pub(super) fn new() -> (Self, mpsc::Receiver<()>) {
        let (tx, rx) = mpsc::channel(1);
        let this = Self {
            tx,
            spawned: Arc::new(AtomicUsize::new(0)),
        };
        (this, rx)
    }

    /// Prepare to spawn a task on the runtime, incrementing the spawned count.
    pub(super) fn reserve(&self) -> Track {
        self.spawned.fetch_add(1, Ordering::Relaxed);
        Track(self.clone())
    }
}

impl Track {
    /// Run a task, decrementing the spawn count when it completes.
    ///
    /// If the spawned count is now 0, this sends a notification on the idle channel.
    pub(super) async fn with<T>(mut self, f: impl Future<Output = T>) -> T {
        let result = f.await;
        let spawned = self.0.spawned.fetch_sub(1, Ordering::Release);
        if spawned == 1 {
            fence(Ordering::Acquire);
            let _ = self.0.tx.send(()).await;
        }
        result
    }
}
