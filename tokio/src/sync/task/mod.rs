//! Thread-safe task notification primitives.

#[cfg(any(feature = "rt", feature = "time"))]
mod atomic_waker;
#[cfg(any(feature = "rt", feature = "time"))]
pub(crate) use self::atomic_waker::AtomicWaker;
