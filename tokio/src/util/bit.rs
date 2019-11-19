#[derive(Debug, Clone, Copy)]
pub(crate) struct Pack {
    mask: usize,
    shift: u32,
}

impl Pack {
    pub(crate) const fn most_significant(width: usize) -> Pack {
        let mask = mask_for(width).reverse_bits();

        Pack {
            mask,
            shift: mask.trailing_zeros(),
        }
    }

    pub(crate) const fn pack(&self, value: usize, base: usize) -> usize {
        (base & !self.mask) | (value << self.shift)
    }

    pub(crate) const fn unpack(&self, src: usize) -> usize {
        unpack(src, self.mask, self.shift)
    }
}

/// Returns a `usize` with the right-most `n` bits set.
pub(crate) const fn mask_for(n: usize) -> usize {
    let shift = 1 << (n - 1);
    shift | (shift - 1)
}

/// Unpack a value using a mask & shift
pub(crate) const fn unpack(src: usize, mask: usize, shift: u32) -> usize {
    (src & mask) >> shift
}
