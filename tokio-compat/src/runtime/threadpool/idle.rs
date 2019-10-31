use std::future::Future;
use std::sync::{
    atomic::{fence, AtomicUsize, Ordering},
    Arc,
};
use tokio_02::sync::mpsc;

#[derive(Clone, Debug)]
pub(super) struct Idle {
    tx: mpsc::Sender<()>,
    spawned: Arc<AtomicUsize>,
}

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

    pub(super) fn reserve(&self) -> Track {
        self.spawned.fetch_add(1, Ordering::Relaxed);
        Track(self.clone())
    }
}

impl Track {
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
