cfg_trace! {
    cfg_rt! {
        pub(crate) use tracing::instrument::Instrumented;

        #[inline]
        #[track_caller]
        pub(crate) fn task<F>(task: F, kind: &'static str, name: Option<&str>) -> Instrumented<F> {
            use tracing::instrument::Instrument;
            let location = std::panic::Location::caller();
            let span = tracing::trace_span!(
                target: "tokio::task",
                "runtime.spawn",
                %kind,
                task.name = %name.unwrap_or_default(),
                loc.file = location.file(),
                loc.line = location.line(),
                loc.col = location.column(),
            );
            task.instrument(span)
        }
    }
}
cfg_time! {
    #[track_caller]
    pub(crate) fn caller_location() -> Option<&'static std::panic::Location<'static>> {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        return Some(std::panic::Location::caller());
        #[cfg(not(all(tokio_unstable, feature = "tracing")))]
        None
    }
}

cfg_not_trace! {
    cfg_rt! {
        #[inline]
        pub(crate) fn task<F>(task: F, _: &'static str, _name: Option<&str>) -> F {
            // nop
            task
        }
    }
}
