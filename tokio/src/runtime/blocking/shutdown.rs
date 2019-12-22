//! A shutdown channel.
//!
//! Each worker holds the `Sender` half. When all the `Sender` halves are
//! dropped, the `Receiver` receives a notification.

use crate::loom::sync::Arc;
use crate::sync::oneshot;

#[derive(Debug, Clone)]
pub(super) struct Sender {
    tx: Arc<oneshot::Sender<()>>,
}

#[derive(Debug)]
pub(super) struct Receiver {
    rx: oneshot::Receiver<()>,
}

pub(super) fn channel() -> (Sender, Receiver) {
    let (tx, rx) = oneshot::channel();
    let tx = Sender { tx: Arc::new(tx) };
    let rx = Receiver { rx };

    (tx, rx)
}

impl Receiver {
    /// Block the current thread until all `Sender` handles drop.
    pub(crate) fn wait(&mut self) {
        use crate::runtime::enter::{enter, try_enter};

        let mut e = if std::thread::panicking() {
            match try_enter() {
                Some(enter) => enter,
                _ => return,
            }
        } else {
            enter()
        };

        // The oneshot completes with an Err
        //
        // If blocking fails to wait, this indicates a problem parking the
        // current thread (usually, shutting down a runtime stored in a
        // thread-local).
        let _ = e.block_on(&mut self.rx);
    }
}
