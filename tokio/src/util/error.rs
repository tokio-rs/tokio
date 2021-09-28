/// Error string explaining that the Tokio context hasn't been instantiated.
pub(crate) const CONTEXT_MISSING_ERROR: &str =
    "there is no reactor running, must be called from the context of a Tokio 1.x runtime";

// some combinations of features might not use this
#[allow(dead_code)]
/// Error string explaining that the Tokio context is shutting down and cannot drive timers.
pub(crate) const RUNTIME_SHUTTING_DOWN_ERROR: &str =
    "A Tokio 1.x context was found, but it is being shutdown.";
