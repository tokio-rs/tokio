//! A shutdown channel.
//!
//! Each worker holds the `Sender` half. When all the `Sender` halves are
//! dropped, the `Receiver` receives a notification.

use crate::loom::sync::Arc;
use crate::sync::oneshot;

use std::time::Duration;

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
    /// Blocks the current thread until all `Sender` handles drop.
    ///
    /// If `timeout` is `Some`, the thread is blocked for **at most** `timeout`
    /// duration. If `timeout` is `None`, then the thread is blocked until the
    /// shutdown signal is received.
    pub(crate) fn wait(&mut self, timeout: Option<Duration>) {
        use crate::runtime::enter::try_enter;

        if timeout == Some(Duration::from_nanos(0)) {
            return;
        }

        let mut e = match try_enter(false) {
            Some(enter) => enter,
            _ => {
                if std::thread::panicking() {
                    // Don't panic in a panic
                    return;
                } else {
                    panic!(
                        "Cannot drop a runtime in a context where blocking is not allowed. \
                        This happens when a runtime is dropped from within an asynchronous context."
                    );
                }
            }
        };

        // The oneshot completes with an Err
        //
        // If blocking fails to wait, this indicates a problem parking the
        // current thread (usually, shutting down a runtime stored in a
        // thread-local).
        if let Some(timeout) = timeout {
            let _ = e.block_on_timeout(&mut self.rx, timeout);
        } else {
            let _ = e.block_on(&mut self.rx);
        }
    }
}
