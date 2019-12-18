use crate::util::bit;
use crate::util::slab::Address;

/// An mutation identifier for a slot in the slab. The generation helps prevent
/// accessing an entry with an outdated token.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct Generation(usize);

impl Generation {
    pub(crate) const WIDTH: u32 = Address::GENERATION_WIDTH;

    pub(super) const MAX: usize = bit::mask_for(Address::GENERATION_WIDTH);

    /// Create a new generation
    ///
    /// # Panics
    ///
    /// Panics if `value` is greater than max generation.
    pub(crate) fn new(value: usize) -> Generation {
        assert!(value <= Self::MAX);
        Generation(value)
    }

    /// Returns the next generation value
    pub(crate) fn next(self) -> Generation {
        Generation((self.0 + 1) & Self::MAX)
    }

    pub(crate) fn to_usize(self) -> usize {
        self.0
    }
}
