extern crate tokio_sync;

fn is_error<T: ::std::error::Error + Send + Sync>() {}

#[test]
fn mpsc_error_bound() {
    use tokio_sync::mpsc::error;

    is_error::<error::RecvError>();
    is_error::<error::SendError>();
    is_error::<error::TrySendError<()>>();
    is_error::<error::UnboundedRecvError>();
    is_error::<error::UnboundedSendError>();
    is_error::<error::UnboundedTrySendError<()>>();
}

#[test]
fn oneshot_error_bound() {
    use tokio_sync::oneshot::error;

    is_error::<error::RecvError>();
    is_error::<error::TryRecvError>();
}

#[test]
fn watch_error_bound() {
    use tokio_sync::watch::error;

    is_error::<error::RecvError>();
    is_error::<error::SendError<()>>();
}
