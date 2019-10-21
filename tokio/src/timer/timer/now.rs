use std::time::Instant;

#[doc(hidden)]
#[deprecated(since = "0.2.4", note = "use clock::Now instead")]
pub trait Now {
    /// Returns an instant corresponding to "now".
    fn now(&mut self) -> Instant;
}

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use crate::clock::Clock as SystemNow;
