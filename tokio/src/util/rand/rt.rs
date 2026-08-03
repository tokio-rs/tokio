use super::{FastRand, RngSeed};

use crate::loom::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering::Relaxed;

/// A deterministic generator for seeds (and other generators).
///
/// Given the same initial seed, the generator will output the same sequence of seeds.
///
#[derive(Debug)]
pub(crate) struct RngSeedGenerator {
    /// Internal state for the seed generator.
    state: AtomicU64,
}

impl RngSeedGenerator {
    /// Returns a new generator from the provided seed.
    pub(crate) fn new(seed: RngSeed) -> Self {
        Self {
            state: AtomicU64::new(pack_seed(seed)),
        }
    }

    /// Returns the next seed in the sequence.
    pub(crate) fn next_seed(&self) -> RngSeed {
        let mut current = self.state.load(Relaxed);

        loop {
            let mut rng = FastRand::from_seed(unpack_seed(current));
            let s = rng.fastrand();
            let r = rng.fastrand();
            let next = pack_state(rng.one, rng.two);

            match self
                .state
                .compare_exchange_weak(current, next, Relaxed, Relaxed)
            {
                Ok(_) => return RngSeed::from_pair(s, r),
                Err(actual) => current = actual,
            }
        }
    }

    /// Directly creates a generator using the next seed.
    pub(crate) fn next_generator(&self) -> Self {
        RngSeedGenerator::new(self.next_seed())
    }
}

fn pack_seed(seed: RngSeed) -> u64 {
    pack_state(seed.s, seed.r)
}

fn pack_state(s: u32, r: u32) -> u64 {
    ((s as u64) << 32) | r as u64
}

fn unpack_seed(seed: u64) -> RngSeed {
    RngSeed::from_pair((seed >> 32) as u32, seed as u32)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_generator_matches_fastrand_sequence() {
        let seed = RngSeed::from_pair(1, 2);
        let generator = RngSeedGenerator::new(seed.clone());
        let mut rng = FastRand::from_seed(seed);

        for _ in 0..8 {
            let expected = RngSeed::from_pair(rng.fastrand(), rng.fastrand());
            let actual = generator.next_seed();

            assert_eq!(actual.s, expected.s);
            assert_eq!(actual.r, expected.r);
        }
    }
}
