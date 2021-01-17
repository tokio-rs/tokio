/// Returns an error string explaining that the Tokio context hasn't been instantiated.
pub(crate) fn context_missing_error() -> String {
    // TODO: Include Tokio version
    String::from(
        "there is no reactor running, must be called from the context of a Tokio 1.x runtime",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_missing_error() {
        assert_eq!(
            &context_missing_error(),
            "there is no reactor running, must be called from the context of a Tokio 1.x runtime"
        );
    }
}
