use crate::loom::future::AtomicWaker;
use crate::loom::sync::atomic::AtomicUsize;
use crate::util::bit;
use crate::util::slab::{Address, Entry, Generation};

use std::sync::atomic::Ordering::{AcqRel, Acquire, SeqCst};

#[derive(Debug)]
pub(crate) struct ScheduledIo {
    readiness: AtomicUsize,
    pub(crate) reader: AtomicWaker,
    pub(crate) writer: AtomicWaker,
}

const PACK: bit::Pack = bit::Pack::most_significant(Generation::WIDTH);

impl Entry for ScheduledIo {
    fn generation(&self) -> Generation {
        unpack_generation(self.readiness.load(SeqCst))
    }

    fn reset(&self, generation: Generation) -> bool {
        let mut current = self.readiness.load(Acquire);

        loop {
            if unpack_generation(current) != generation {
                return false;
            }

            let next = PACK.pack(generation.next().to_usize(), 0);

            match self
                .readiness
                .compare_exchange(current, next, AcqRel, Acquire)
            {
                Ok(_) => break,
                Err(actual) => current = actual,
            }
        }

        drop(self.reader.take_waker());
        drop(self.writer.take_waker());

        true
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
    #[cfg(all(test, loom))]
    /// Returns the current readiness value of this `ScheduledIo`, if the
    /// provided `token` is still a valid access.
    ///
    /// # Returns
    ///
    /// If the given token's generation no longer matches the `ScheduledIo`'s
    /// generation, then the corresponding IO resource has been removed and
    /// replaced with a new resource. In that case, this method returns `None`.
    /// Otherwise, this returns the current readiness.
    pub(crate) fn get_readiness(&self, address: Address) -> Option<usize> {
        let ready = self.readiness.load(Acquire);

        if unpack_generation(ready) != address.generation() {
            return None;
        }

        Some(ready & !PACK.mask())
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
        address: Address,
        f: impl Fn(usize) -> usize,
    ) -> Result<usize, ()> {
        let generation = address.generation();

        let mut current = self.readiness.load(Acquire);

        loop {
            // Check that the generation for this access is still the current
            // one.
            if unpack_generation(current) != generation {
                return Err(());
            }
            // Mask out the generation bits so that the modifying function
            // doesn't see them.
            let current_readiness = current & mio::Ready::all().as_usize();
            let new = f(current_readiness);

            debug_assert!(
                new <= !PACK.max_value(),
                "new readiness value would overwrite generation bits!"
            );

            match self.readiness.compare_exchange(
                current,
                PACK.pack(generation.to_usize(), new),
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

fn unpack_generation(src: usize) -> Generation {
    Generation::new(PACK.unpack(src))
}
