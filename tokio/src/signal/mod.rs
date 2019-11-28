//! Asynchronous signal handling for Tokio
//!
//! Note that signal handling is in general a very tricky topic and should be
//! used with great care. This crate attempts to implement 'best practice' for
//! signal handling, but it should be evaluated for your own applications' needs
//! to see if it's suitable.
//!
//! The are some fundamental limitations of this crate documented on the OS
//! specific structures, as well.
//!
//! # Examples
//!
//! Print on "ctrl-c" notification.
//!
//! ```rust,no_run
//! use tokio::signal;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     signal::ctrl_c().await?;
//!     println!("ctrl-c received!");
//!     Ok(())
//! }
//! ```
//!
//! Wait for SIGHUP on Unix
//!
//! ```rust,no_run
//! # #[cfg(unix)] {
//! use tokio::signal::unix::{signal, SignalKind};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // An infinite stream of hangup signals.
//!     let mut stream = signal(SignalKind::hangup())?;
//!
//!     // Print whenever a HUP signal is received
//!     loop {
//!         stream.recv().await;
//!         println!("got signal HUP");
//!     }
//! }
//! # }
//! ```

mod ctrl_c;
pub use ctrl_c::ctrl_c;

mod registry;

mod os {
    #[cfg(unix)]
    pub(crate) use super::unix::{OsExtraData, OsStorage};

    #[cfg(windows)]
    pub(crate) use super::windows::{OsExtraData, OsStorage};
}

pub mod unix;
pub mod windows;
