/// Unwrap a ready value or propagate `Async::Pending`.
#[macro_export]
macro_rules! ready {
    ($e:expr) => {{
        use std::task::Poll::{Ready, Pending};

        match $e {
            Ready(v) => v,
            Pending => return Pending,
        }
    }}
}
