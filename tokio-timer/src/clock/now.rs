use std::time::Instant;

/// Returns [`Instant`] values representing the current instant in time.
///
/// This allows customizing the source of time which is especially useful for
/// testing.
///
/// Implementations must ensure that calls to `now` return monotonically
/// increasing [`Instant`] values.
///
/// [`Instant`]: https://doc.rust-lang.org/std/time/struct.Instant.html
pub trait Now: Send + Sync + 'static {
    /// Returns an instant corresponding to "now".
    fn now(&self) -> Instant;
}
