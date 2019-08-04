//! OS-specific functionality.

#[cfg(unix)]
pub mod unix;

#[cfg(windows)]
pub mod windows;
