//! This module provides a small facade that wraps the `tracing` APIs we use, so
//! that when the `tracing` dependency is disabled, `tracing`'s macros expand to
//! no-ops.
//!
//! This means we don't have to put a `#[cfg(feature = "tracing")]` on every
//! individual use of a `tracing` macro.
#[cfg(feature = "tracing")]
macro_rules! trace {
    ($($arg:tt)+) => {
        tracing::trace!($($arg)+)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($($arg:tt)+) => {};
}

#[cfg(feature = "tracing")]
macro_rules! debug {
    ($($arg:tt)+) => {
        tracing::debug!($($arg)+)
    };
}

#[cfg(not(feature = "tracing"))]
macro_rules! debug {
    ($($arg:tt)+) => {};
}
