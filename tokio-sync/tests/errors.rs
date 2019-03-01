#![deny(warnings)]

extern crate tokio_sync;

use tokio_sync::mpsc::error;

fn is_error<T: ::std::error::Error + Send + Sync>() {}

#[test]
fn error_bound() {
    is_error::<error::RecvError>();
    is_error::<error::SendError>();
    is_error::<error::TrySendError<()>>();
    is_error::<error::UnboundedRecvError>();
    is_error::<error::UnboundedSendError>();
    is_error::<error::UnboundedTrySendError<()>>();
}
