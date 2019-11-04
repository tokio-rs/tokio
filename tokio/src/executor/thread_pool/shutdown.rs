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
        use crate::executor::enter;

        let mut e = match enter() {
            Ok(e) => e,
            Err(_) => {
                if std::thread::panicking() {
                    // Already panicking, avoid a double panic
                    return;
                } else {
                    panic!("cannot block on shutdown from the Tokio runtime");
                }
            }
        };

        // The oneshot completes with an Err
        let _ = e.block_on(&mut self.rx);
    }
}
