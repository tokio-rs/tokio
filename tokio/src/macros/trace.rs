cfg_trace! {
    macro_rules! trace_op {
        ($name:literal, $readiness:literal, $parent:expr) => {
            tracing::trace!(
                target: "runtime::resource::poll_op",
                parent: $parent,
                op_name = $name,
                is_ready = $readiness
            );
        }
    }

    macro_rules! trace_poll_op {
        ($name:literal, $poll:expr, $parent:expr $(,)*) => {
            match $poll {
                std::task::Poll::Ready(t) => {
                    trace_op!($name, true, $parent);
                    std::task::Poll::Ready(t)
                }
                std::task::Poll::Pending => {
                    trace_op!($name, false, $parent);
                    return std::task::Poll::Pending;
                }
            }
        };
    }
}
