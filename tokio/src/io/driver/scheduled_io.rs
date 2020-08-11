use crate::loom::future::AtomicWaker;
use crate::loom::sync::atomic::AtomicUsize;
use crate::util::slab::Entry;

use std::sync::atomic::Ordering::{AcqRel, Acquire, Release};

/// Stored in the I/O driver resource slab.
#[derive(Debug)]
pub(crate) struct ScheduledIo {
    /// Packs the resource's readiness with the resource's generation.
    readiness: AtomicUsize,

    /// Task waiting on read readiness
    pub(crate) reader: AtomicWaker,

    /// Task waiting on write readiness
    pub(crate) writer: AtomicWaker,
}

impl Entry for ScheduledIo {
    fn reset(&self) {
        let state = self.readiness.load(Acquire);

        let generation = super::GENERATION.unpack(state);
        let next = super::GENERATION.pack_lossy(generation + 1, 0);

        self.readiness.store(next, Release);
    }
}

impl Default for ScheduledIo {
    fn default() -> ScheduledIo {
        ScheduledIo {
            readiness: AtomicUsize::new(0),
            reader: AtomicWaker::new(),
            writer: AtomicWaker::new(),
        }
    }
}

impl ScheduledIo {
    pub(crate) fn generation(&self) -> usize {
        super::GENERATION.unpack(self.readiness.load(Acquire))
    }

    /// Sets the readiness on this `ScheduledIo` by invoking the given closure on
    /// the current value, returning the previous readiness value.
    ///
    /// # Arguments
    /// - `token`: the token for this `ScheduledIo`.
    /// - `f`: a closure returning a new readiness value given the previous
    ///   readiness.
    ///
    /// # Returns
    ///
    /// If the given token's generation no longer matches the `ScheduledIo`'s
    /// generation, then the corresponding IO resource has been removed and
    /// replaced with a new resource. In that case, this method returns `Err`.
    /// Otherwise, this returns the previous readiness.
    pub(crate) fn set_readiness(
        &self,
        token: Option<usize>,
        f: impl Fn(usize) -> usize,
    ) -> Result<usize, ()> {
        let mut current = self.readiness.load(Acquire);

        loop {
            let current_generation = super::GENERATION.unpack(current);

            if let Some(token) = token {
                // Check that the generation for this access is still the
                // current one.
                if super::GENERATION.unpack(token) != current_generation {
                    return Err(());
                }
            }

            // Mask out the generation bits so that the modifying function
            // doesn't see them.
            let current_readiness = current & mio::Ready::all().as_usize();
            let new = f(current_readiness);

            debug_assert!(
                new <= super::ADDRESS.max_value(),
                "new readiness value would overwrite generation bits!"
            );

            match self.readiness.compare_exchange(
                current,
                super::GENERATION.pack(current_generation, new),
                AcqRel,
                Acquire,
            ) {
                Ok(_) => return Ok(current),
                // we lost the race, retry!
                Err(actual) => current = actual,
            }
        }
    }
}

impl Drop for ScheduledIo {
    fn drop(&mut self) {
        self.writer.wake();
        self.reader.wake();
    }
}
