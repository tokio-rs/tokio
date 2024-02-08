macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            #[allow(clippy::blocks_in_conditions)]
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}
