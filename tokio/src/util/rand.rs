use std::cell::Cell;

cfg_rt! {
    use std::sync::Mutex;

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
                state: Mutex::new(FastRand::new(seed)),
            }
        }

        /// Returns the next seed in the sequence.
        pub(crate) fn next_seed(&self) -> RngSeed {
            let rng = self
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
}

/// A seed for random number generation.
///
/// In order to make certain functions within a runtime deterministic, a seed
/// can be specified at the time of creation.
#[allow(unreachable_pub)]
#[derive(Clone, Debug)]
pub struct RngSeed {
    s: u32,
    r: u32,
}

impl RngSeed {
    /// Creates a random seed using loom internally.
    pub(crate) fn new() -> Self {
        Self::from_u64(crate::loom::rand::seed())
    }

    cfg_unstable! {
        /// Generates a seed from the provided byte slice.
        ///
        /// # Example
        ///
        /// ```
        /// # use tokio::runtime::RngSeed;
        /// let seed = RngSeed::from_bytes(b"make me a seed");
        /// ```
        #[cfg(feature = "rt")]
        pub fn from_bytes(bytes: &[u8]) -> Self {
            use std::{collections::hash_map::DefaultHasher, hash::Hasher};

            let mut hasher = DefaultHasher::default();
            hasher.write(bytes);
            Self::from_u64(hasher.finish())
        }
    }

    fn from_u64(seed: u64) -> Self {
        let one = (seed >> 32) as u32;
        let mut two = seed as u32;

        if two == 0 {
            // This value cannot be zero
            two = 1;
        }

        Self::from_pair(one, two)
    }

    fn from_pair(s: u32, r: u32) -> Self {
        Self { s, r }
    }
}
/// Fast random number generate.
///
/// Implement xorshift64+: 2 32-bit xorshift sequences added together.
/// Shift triplet `[17,7,16]` was calculated as indicated in Marsaglia's
/// Xorshift paper: <https://www.jstatsoft.org/article/view/v008i14/xorshift.pdf>
/// This generator passes the SmallCrush suite, part of TestU01 framework:
/// <http://simul.iro.umontreal.ca/testu01/tu01.html>
#[derive(Debug)]
pub(crate) struct FastRand {
    one: Cell<u32>,
    two: Cell<u32>,
}

impl FastRand {
    /// Initializes a new, thread-local, fast random number generator.
    pub(crate) fn new(seed: RngSeed) -> FastRand {
        FastRand {
            one: Cell::new(seed.s),
            two: Cell::new(seed.r),
        }
    }

    /// Replaces the state of the random number generator with the provided seed, returning
    /// the seed that represents the previous state of the random number generator.
    ///
    /// The random number generator will become equivalent to one created with
    /// the same seed.
    #[cfg(feature = "rt")]
    pub(crate) fn replace_seed(&self, seed: RngSeed) -> RngSeed {
        let old_seed = RngSeed::from_pair(self.one.get(), self.two.get());

        self.one.replace(seed.s);
        self.two.replace(seed.r);

        old_seed
    }

    #[cfg(any(
        feature = "macros",
        feature = "rt-multi-thread",
        all(feature = "sync", feature = "rt")
    ))]
    pub(crate) fn fastrand_n(&self, n: u32) -> u32 {
        // This is similar to fastrand() % n, but faster.
        // See https://lemire.me/blog/2016/06/27/a-fast-alternative-to-the-modulo-reduction/
        let mul = (self.fastrand() as u64).wrapping_mul(n as u64);
        (mul >> 32) as u32
    }

    fn fastrand(&self) -> u32 {
        let mut s1 = self.one.get();
        let s0 = self.two.get();

        s1 ^= s1 << 17;
        s1 = s1 ^ s0 ^ s1 >> 7 ^ s0 >> 16;

        self.one.set(s0);
        self.two.set(s1);

        s0.wrapping_add(s1)
    }
}
