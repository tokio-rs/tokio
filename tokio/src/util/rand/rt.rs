use super::{FastRand, RngSeed};

use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;
use std::sync::Mutex;

impl RngSeed {
    /// Generates a seed from the provided byte slice.
    ///
    /// # Example
    ///
    /// ```
    /// # use tokio::runtime::RngSeed;
    /// let seed = RngSeed::from_bytes(b"make me a seed");
    /// ```
    #[allow(unreachable_pub)]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut hasher = DefaultHasher::default();
        hasher.write(bytes);
        Self::from_u64(hasher.finish())
    }
}

/// A deterministic generator for seeds (and other generators).
///
/// Given the same initial seed, the generator will output the same sequence of seeds.
///
/// Since the seed generator will be kept in a runtime handle, we need to wrap `FastRand`
/// in a Mutex to make it thread safe. Different to the `FastRand` that we keep in a
/// thread local store, the expectation is that seed generation will not need to happen
/// very frequently, so the cost of the mutex should be minimal.
#[derive(Debug)]
pub(crate) struct RngSeedGenerator {
    /// Internal state for the seed generator. We keep it in a Mutex so that we can safely
    /// use it across multiple threads.
    state: Mutex<FastRand>,
}

impl RngSeedGenerator {
    /// Returns a new generator from the provided seed.
    pub(crate) fn new(seed: RngSeed) -> Self {
        Self {
            state: Mutex::new(FastRand::from_seed(seed)),
        }
    }

    /// Returns the next seed in the sequence.
    pub(crate) fn next_seed(&self) -> RngSeed {
        let mut rng = self
            .state
            .lock()
            .expect("RNG seed generator is internally corrupt");

        let s = rng.fastrand();
        let r = rng.fastrand();

        RngSeed::from_pair(s, r)
    }

    /// Directly creates a generator using the next seed.
    pub(crate) fn next_generator(&self) -> Self {
        RngSeedGenerator::new(self.next_seed())
    }
}

impl FastRand {
    /// Replaces the state of the random number generator with the provided seed, returning
    /// the seed that represents the previous state of the random number generator.
    ///
    /// The random number generator will become equivalent to one created with
    /// the same seed.
    pub(crate) fn replace_seed(&mut self, seed: RngSeed) -> RngSeed {
        let old_seed = RngSeed::from_pair(self.one, self.two);

        self.one = seed.s;
        self.two = seed.r;

        old_seed
    }
}
