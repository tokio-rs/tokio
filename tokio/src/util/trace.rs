cfg_trace! {
    cfg_rt! {
        pub(crate) use tracing::instrument::Instrumented;

        #[inline]
        pub(crate) fn task<F>(task: F, kind: &'static str) -> Instrumented<F> {
            use tracing::instrument::Instrument;
            task.instrument(tracing::trace_span!(
                target: "tokio::task",
                "task",
                %kind,
            ))
        }
    }
}

cfg_not_trace! {
    cfg_rt! {
        #[inline]
        pub(crate) fn task<F>(task: F, _: &'static str) -> F {
            // nop
            task
        }
    }
}
