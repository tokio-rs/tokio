/// Error string explaining that the Tokio context hasn't been instantiated.
pub(crate) const CONTEXT_MISSING_ERROR: &str =
    "there is no reactor running, must be called from the context of a Tokio 1.x runtime";
