/// Unwrap a ready value or propagate `Async::Pending`.
#[macro_export]
macro_rules! ready {
    ($e:expr) => {{
        use std::task::Poll::{Pending, Ready};

        match $e {
            Ready(v) => v,
            Pending => return Pending,
        }
    }};
}

/// A macro for extracting the successful type of a `Poll<T, E>`.
///
/// This macro bakes propagation of both errors and `Pending` signals by
/// returning early.
#[macro_export]
macro_rules! try_ready {
    ($e:expr) => {
        match $e {
            std::task::Poll::Pending => return std::task::Poll::Pending,
            std::task::Poll::Ready(Err(e)) => return std::task::Poll::Ready(Err(From::from(e))),
            std::task::Poll::Ready(Ok(t)) => t,
        }
    };
}
