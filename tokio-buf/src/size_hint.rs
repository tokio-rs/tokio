use std::u64;

/// A `BufStream` size hint
///
/// The default implementation returns:
///
/// * 0 for `available`
/// * 0 for `lower`
/// * `None` for `upper`.
#[derive(Debug, Default, Clone)]
pub struct SizeHint {
    lower: u64,
    upper: Option<u64>,
}

impl SizeHint {
    /// Returns a new `SizeHint` with default values
    pub fn new() -> SizeHint {
        SizeHint::default()
    }

    /// Returns the lower bound of data that the `BufStream` will yield before
    /// completing.
    pub fn lower(&self) -> u64 {
        self.lower
    }

    /// Set the value of the `lower` hint.
    ///
    /// # Panics
    ///
    /// The function panics if `value` is greater than `upper`.
    pub fn set_lower(&mut self, value: u64) {
        assert!(value <= self.upper.unwrap_or(u64::MAX));
        self.lower = value;
    }

    /// Returns the upper bound of data the `BufStream` will yield before
    /// completing, or `None` if the value is unknown.
    pub fn upper(&self) -> Option<u64> {
        self.upper
    }

    /// Set the value of the `upper` hint value.
    ///
    /// # Panics
    ///
    /// This function panics if `value` is less than `lower`.
    pub fn set_upper(&mut self, value: u64) {
        // There is no need to check `available` as that is guaranteed to be
        // less than or equal to `lower`.
        assert!(value >= self.lower, "`value` is less than than `lower`");

        self.upper = Some(value);
    }
}
