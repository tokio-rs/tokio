use std::time::Instant;

#[doc(hidden)]
#[deprecated(since = "0.2.4", note = "use clock::Now instead")]
pub trait Now {
    /// Returns an instant corresponding to "now".
    fn now(&mut self) -> Instant;
}

pub use clock::Clock as SystemNow;
