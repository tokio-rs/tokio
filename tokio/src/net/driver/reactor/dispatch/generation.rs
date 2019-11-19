use super::{Pack, Tid, RESERVED_BITS, WIDTH};

/// An mutation identifier for a slot in the slab. The generation helps prevent
/// accessing an entry with an outdated token.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct Generation {
    value: usize,
}

impl Generation {
    pub(crate) const WIDTH: usize = <Self as Pack>::LEN;

    /// Create a new generation
    ///
    /// # Panics
    ///
    /// Panics if `value` is greater than max generation.
    pub(crate) fn new(value: usize) -> Generation {
        assert!(value <= Self::BITS);

        Generation { value }
    }

    /// Returns the next generation value
    pub(crate) fn next(self) -> Generation {
        Generation::from_usize((self.value + 1) % Self::BITS)
    }

    pub(crate) fn to_usize(self) -> usize {
        self.value
    }
}

impl Pack for Generation {
    /// Use all the remaining bits in the word for the generation counter, minus
    /// any bits reserved by the user.
    const LEN: usize = (WIDTH - RESERVED_BITS) - Self::SHIFT;

    type Prev = Tid;

    #[inline(always)]
    fn from_usize(u: usize) -> Self {
        debug_assert!(u <= Self::BITS);
        Self::new(u)
    }

    #[inline(always)]
    fn as_usize(&self) -> usize {
        self.value
    }
}
