cfg_trace! {
    cfg_rt! {
        pub(crate) use tracing::instrument::Instrumented;

        #[inline]
        #[cfg_attr(tokio_track_caller, track_caller)]
        pub(crate) fn task<F>(task: F, kind: &'static str) -> Instrumented<F> {
            use tracing::instrument::Instrument;
            #[cfg(tokio_track_caller)]
            let location = std::panic::Location::caller();
            #[cfg(tokio_track_caller)]
            let span = tracing::trace_span!(
                target: "tokio::task",
                "task",
                %kind,
                spawn.location = %format_args!("{}:{}:{}", location.file(), location.line(), location.column()),
            );
            #[cfg(not(tokio_track_caller))]
            let span = tracing::trace_span!(
                target: "tokio::task",
                "task",
                %kind,
            );
            task.instrument(span)
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
