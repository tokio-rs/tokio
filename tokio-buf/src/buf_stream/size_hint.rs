/// A `BufStream` size hint
///
/// The default implementation returns:
///
/// * 0 for `available`
/// * 0 for `lower`
/// * `None` for `upper`.
#[derive(Debug, Default, Clone)]
pub struct SizeHint {
    available: usize,
    lower: usize,
    upper: Option<usize>,
}

impl SizeHint {
    /// Returns a new `SizeHint` with default values
    pub fn new() -> SizeHint {
        SizeHint::default()
    }

    /// Returns the **lower bound** of the amount of data that can be read from
    /// the `BufStream` without `NotReady` being returned.
    ///
    /// It is possible that more data is currently available.
    pub fn available(&self) -> usize {
        self.available
    }

    /// Set the value of the `available` hint.
    pub fn set_available(&mut self, value: usize) {
        self.available = value;

        if self.lower < value {
            self.lower = value;

            match self.upper {
                Some(ref mut upper) if *upper < value => {
                    *upper = value;
                }
                _ => {}
            }
        }
    }

    /// Returns the lower bound of data that the `BufStream` will yield before
    /// completing.
    pub fn lower(&self) -> usize {
        self.lower
    }

    /// Set the value of the `lower` hint.
    ///
    /// # Panics
    ///
    /// This function panics if `value` is less than `available`.
    pub fn set_lower(&mut self, value: usize) {
        assert!(value >= self.available);
        self.lower = value;
    }

    /// Returns the upper bound of data the `BufStream` will yield before
    /// completing, or `None` if the value is unknown.
    pub fn upper(&self) -> Option<usize> {
        self.upper
    }

    /// Set the value of the `upper` hint value.
    ///
    /// # Panics
    ///
    /// This function panics if `value` is less than `lower`.
    pub fn set_upper(&mut self, value: usize) {
        // There is no need to check `available` as that is guaranteed to be
        // less than or equal to `lower`.
        assert!(value >= self.lower, "`value` is less than than `lower`");

        self.upper = Some(value);
    }
}
