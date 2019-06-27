// TODO this macro should probably be somewhere in tokio-futures
/// A macro for extracting the successful type of a `Poll<T, E>`.
///
/// This macro bakes propagation of both errors and `Pending` signals by
/// returning early.
macro_rules! try_ready {
    ($e:expr) => {
        match $e {
            std::task::Poll::Pending => return std::task::Poll::Pending,
            std::task::Poll::Ready(Err(e)) => return std::task::Poll::Ready(Err(From::from(e))),
            std::task::Poll::Ready(Ok(t)) => t,
        }
    };
}

/// A macro to reduce some of the boilerplate for projecting from
/// `Pin<&mut T>` to `Pin<&mut T.field>`
macro_rules! pin {
    ($e:expr) => {
        std::pin::Pin::new(&mut $e)
    };
}
