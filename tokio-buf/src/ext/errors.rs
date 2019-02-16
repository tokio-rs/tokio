//! Error types

pub use super::collect::CollectError;
pub use super::from::CollectVecError;
pub use super::limit::LimitError;

// Being crate-private, we should be able to swap the type out in a
// backwards compatible way.
pub(crate) mod internal {
    use std::{error, fmt};

    /// An error that can never occur
    pub enum Never {}

    impl fmt::Debug for Never {
        fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
            match *self {}
        }
    }

    impl fmt::Display for Never {
        fn fmt(&self, _f: &mut fmt::Formatter) -> fmt::Result {
            match *self {}
        }
    }

    impl error::Error for Never {
        fn description(&self) -> &str {
            match *self {}
        }
    }
}
