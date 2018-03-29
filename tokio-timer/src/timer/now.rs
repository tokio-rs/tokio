use std::time::Instant;

/// Returns `Instant` values representing the current instant in time.
///
/// This allows customizing the source of time which is especially useful for
/// testing.
pub trait Now {
    /// Returns an instant corresponding to "now".
    fn now(&mut self) -> Instant;
}

/// Returns the instant corresponding to now using a monotonic clock.
#[derive(Debug)]
pub struct SystemNow(());

impl SystemNow {
    /// Create a new `SystemNow`.
    pub fn new() -> SystemNow {
        SystemNow(())
    }
}

impl Now for SystemNow {
    fn now(&mut self) -> Instant {
        Instant::now()
    }
}
