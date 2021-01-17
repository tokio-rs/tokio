#[cfg(any(feature = "rt", feature = "net"))]
use std::fmt::Write;

#[cfg(any(feature = "rt", feature = "net"))]
/// Returns an error string explaining that the Tokio context hasn't been instantiated.
pub(crate) fn context_missing_error(features: &[&str]) -> String {
    // TODO: Include Tokio version
    let sfx = if !features.is_empty() {
        let mut sfx = String::from(" with ");
        for (i, feat) in features.iter().enumerate() {
            if i == 0 {
                if features.len() > 1 {
                    write!(&mut sfx, "either ").expect("failed to write to string");
                }
            } else if i == features.len() - 1 {
                write!(&mut sfx, " or ").expect("failed to write to string");
            } else {
                write!(&mut sfx, ", ").expect("failed to write to string");
            }
            write!(&mut sfx, "{}", feat).expect("failed to write to string");
        }
        write!(&mut sfx, " enabled").expect("failed to write to string");
        sfx
    } else {
        String::new()
    };
    format!(
        "there is no reactor running, must be called from the context of Tokio 1.x runtime{}",
        sfx
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_missing_error_no_features() {
        assert_eq!(
            &context_missing_error(&[]),
            "there is no reactor running, must be called from the context of Tokio 1.x runtime"
        );
    }

    #[test]
    fn test_context_missing_error_one_feature() {
        assert_eq!(&context_missing_error(&["rt"]), 
                   "there is no reactor running, must be called from the context of Tokio 1.x runtime with rt enabled");
    }

    #[test]
    fn test_context_missing_error_two_features() {
        assert_eq!(&context_missing_error(&["rt", "signal"]), 
                "there is no reactor running, must be called from the context of Tokio 1.x runtime with either rt or signal enabled");
    }

    #[test]
    fn test_context_missing_error_three_features() {
        assert_eq!(&context_missing_error(&["rt", "signal", "sync"]), 
                   "there is no reactor running, must be called from the context of Tokio 1.x runtime with either rt, signal or sync enabled"
                );
    }
}
