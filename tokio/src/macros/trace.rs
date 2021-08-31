cfg_trace! {
    macro_rules! trace_op {
        ($name:literal, $readiness:literal) => {
            tracing::trace!(
                target: "runtime::resource::poll_op",
                op_name = $name,
                is_ready = $readiness
            );
        }
    }

    macro_rules! trace_poll_op {
        ($name:literal, $poll:expr) => {
            match $poll {
                std::task::Poll::Ready(t) => {
                    trace_op!($name, true);
                    std::task::Poll::Ready(t)
                }
                std::task::Poll::Pending => {
                    trace_op!($name, false);
                    return std::task::Poll::Pending;
                }
            }
        };
    }
}
