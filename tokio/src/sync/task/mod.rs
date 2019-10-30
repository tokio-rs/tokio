//! Thread-safe task notification primitives.

mod atomic_waker;
pub use self::atomic_waker::AtomicWaker;
