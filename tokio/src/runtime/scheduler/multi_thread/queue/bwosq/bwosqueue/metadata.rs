//! Contains metadata for the block configuration

use crate::loom::sync::atomic::{AtomicUsize, Ordering};
use core::fmt::{Debug, Formatter};

/// A container for the current block index and block version
///
/// `NE` is the number of elements in a block (index `0..NE`). `index == NE` marks a full block.
///
/// Bits `0..=NE_LOG_CEIL`, where `NE_LOG_CEIL` is `(NE+1).next_power_of_two()).log2()`
/// are reserved for the index.
/// Bits `(NE_LOG_CEIL + 1)..` are used for the block version. The version field is
/// used to detect [ABA](https://en.wikipedia.org/wiki/ABA_problem) situations when accessing queue entries.
#[repr(transparent)]
#[derive(PartialEq, Eq, Copy, Clone)]
pub(crate) struct IndexAndVersion<const NE: usize>(usize);

/// The index of the current element in the block
///
/// 0 represents an empty block while NE represents a full block.
#[repr(transparent)]
pub(crate) struct Index<const NE: usize>(usize);

impl<const NUM_ELEMENTS_PER_BLOCK: usize> Index<NUM_ELEMENTS_PER_BLOCK> {
    /// Creates an Index for an empty block
    #[inline(always)]
    pub(crate) fn empty() -> Self {
        Self(0)
    }

    /// Creates an Index for a full block
    #[inline(always)]
    pub(crate) fn full() -> Self {
        Self(NUM_ELEMENTS_PER_BLOCK)
    }

    /// True if the block is full
    #[inline(always)]
    pub(crate) fn is_full(&self) -> bool {
        self.0 == NUM_ELEMENTS_PER_BLOCK
    }

    /// True if the block is empty
    #[inline(always)]
    #[allow(dead_code)]
    pub(crate) fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

// todo: use atomic usize after fixing overflow problem to support 32bit
#[repr(transparent)]
pub(crate) struct AtomicIndexAndVersion<const NE: usize>(AtomicUsize);

impl<const NE: usize> IndexAndVersion<{ NE }> {
    // 0 elements per block make no sense
    #[cfg(const_assert)]
    const _ASSERT_NE_GREATER_ZERO: () = assert!(NE > 0);
    #[cfg(const_assert)]
    const MIN_VERSION_BITS: u32 = 1;
    // Subtract 1 to get the maximum number representable by that amount of bits and subtract another one to allow for
    // representing the full block state (`idx == NE`).
    #[cfg(const_assert)]
    const MAX_NE: usize = 2_usize.pow(usize::BITS - Self::MIN_VERSION_BITS) - 2;
    #[cfg(const_assert)]
    const _ASSERT_NE_MAX: () = assert!(NE <= Self::MAX_NE);

    #[inline(always)]
    fn raw(&self) -> usize {
        self.0
    }

    #[inline(always)]
    fn max_version() -> usize {
        let num_version_bits = usize::BITS - Self::ne_log() as u32;
        2_usize.pow(num_version_bits).wrapping_sub(1)
    }

    /// Number of bits used for the Number of elements in a block
    ///
    /// Guaranteed to be at least 1.
    #[inline]
    fn ne_log() -> usize {
        #[cfg(const_assert)]
        #[allow(clippy::let_unit_value)]
        let _ = Self::_ASSERT_NE_GREATER_ZERO;
        #[cfg(const_assert)]
        #[allow(clippy::let_unit_value)]
        let _ = Self::_ASSERT_NE_MAX;
        // (const) integer logarithm is not stable yet, so we need to use floating point and
        // rely on the compiler to optimize this away at compile time.
        ((NE + 1).next_power_of_two() as f32).log2() as usize
    }

    #[inline(always)]
    pub(crate) fn new(version: usize, index: Index<NE>) -> Self {
        debug_assert!(version <= Self::max_version());

        Self(version.wrapping_shl(Self::ne_log() as u32) | index.0)
    }

    #[inline(always)]
    fn from_raw(raw: usize) -> Self {
        Self(raw)
    }

    #[inline(always)]
    pub(crate) fn version(&self) -> usize {
        self.0.wrapping_shr(Self::ne_log() as u32)
    }

    /// Increment the version by one if this is the first block and reset index
    #[inline]
    pub(crate) fn next_version(&self, is_first_block: bool) -> Self {
        let cur_version_shifted = self.0 & Self::version_mask();
        let first_bit_pos_version = Self::ne_log() as u32;
        let new_version_shifted = cur_version_shifted
            .wrapping_add((is_first_block as usize).wrapping_shl(first_bit_pos_version));
        // index is now zeroed.
        Self(new_version_shifted)
    }

    /// A bitmask for the bits used for the block index
    #[inline(always)]
    fn index_mask() -> usize {
        // ne_log will be at least 1, so the subtraction will never wrap around
        1_usize.wrapping_shl(Self::ne_log() as u32) - 1
    }

    #[inline(always)]
    fn version_mask() -> usize {
        !Self::index_mask()
    }

    #[inline(always)]
    pub(crate) fn index(&self) -> Index<NE> {
        // We are sure that the index we stored is valid
        Index(self.raw_index())
    }

    #[inline(always)]
    pub(crate) fn raw_index(&self) -> usize {
        self.0 & Self::index_mask()
    }

    /// Increment the Index by `rhs`.
    ///
    /// # Safety
    ///
    /// The caller be sure that the result of self + rhs is <= NE.
    #[inline(always)]
    pub(crate) unsafe fn index_add_unchecked(&self, rhs: usize) -> Self {
        debug_assert!(self.raw_index() + rhs <= NE);
        Self(self.0.wrapping_add(rhs))
    }
}

impl<const NE: usize> Debug for IndexAndVersion<{ NE }> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IndexAndVersion")
            .field("Index", &self.raw_index())
            .field("Version", &self.version())
            .finish()
    }
}

impl<const NE: usize> AtomicIndexAndVersion<{ NE }> {
    #[inline(always)]
    pub(crate) fn load(&self, order: Ordering) -> IndexAndVersion<NE> {
        let v = self.0.load(order);
        IndexAndVersion::from_raw(v)
    }

    /// Creates a new instance for an `Owner` field (producer or consumer
    pub(crate) fn new_owner(is_queue_head: bool) -> Self {
        let empty_val: IndexAndVersion<NE> = if is_queue_head {
            // The first block (head) starts at version one and with an empty index
            // to indicate readiness to produce/consume once values where produced.
            IndexAndVersion::new(1, Index::empty())
        } else {
            // The remaining blocks start one version behind and are marked as fully
            // produced/consumed.
            IndexAndVersion::new(0, Index::full())
        };
        Self(AtomicUsize::new(empty_val.raw()))
    }

    /// Creates a new instance for a `Stealer` field. The main difference to
    /// [new_owner](Self::new_owner) is that the stealer is always initialized as full,
    /// i.e. not ready for stealing. This is because the queue head is reserved for the
    /// consumer and the stealer may not steal from the same block the consumer is on.
    pub(crate) fn new_stealer(is_queue_head: bool) -> Self {
        let full_val: IndexAndVersion<NE> =
            IndexAndVersion::new(is_queue_head as usize, Index::full());
        Self(AtomicUsize::new(full_val.raw()))
    }

    #[inline(always)]
    pub(crate) fn fetch_add(&self, val: usize, order: Ordering) -> IndexAndVersion<NE> {
        let old = self.0.fetch_add(val, order);
        IndexAndVersion::from_raw(old)
    }

    #[inline(always)]
    pub(crate) fn compare_exchange_weak(
        &self,
        current: IndexAndVersion<NE>,
        new: IndexAndVersion<NE>,
        success: Ordering,
        failure: Ordering,
    ) -> Result<IndexAndVersion<NE>, IndexAndVersion<NE>> {
        self.0
            .compare_exchange_weak(current.raw(), new.raw(), success, failure)
            .map_err(IndexAndVersion::from_raw)
            .map(IndexAndVersion::from_raw)
    }

    #[inline(always)]
    pub(crate) fn store(&self, val: IndexAndVersion<NE>, order: Ordering) {
        self.0.store(val.raw(), order)
    }

    #[inline(always)]
    pub(crate) fn swap(&self, val: IndexAndVersion<NE>, order: Ordering) -> IndexAndVersion<NE> {
        let old = self.0.swap(val.raw(), order);
        IndexAndVersion::from_raw(old)
    }
}

impl<const NE: usize> Debug for AtomicIndexAndVersion<{ NE }> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let val = self.load(Ordering::SeqCst);
        f.write_fmt(format_args!("{:?}", val))
    }
}
