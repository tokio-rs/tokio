#[cfg(unix)]
use super::unix::{self as os_impl};
#[cfg(windows)]
use super::windows::{self as os_impl};

use std::io;

/// Completes when a "ctrl-c" notification is sent to the process.
///
/// In general signals are handled very differently across Unix and Windows, but
/// this is somewhat cross platform in terms of how it can be handled. A ctrl-c
/// event to a console process can be represented as a stream for both Windows
/// and Unix.
///
/// Note that there are a number of caveats listening for signals, and you may
/// wish to read up on the documentation in the `unix` or `windows` module to
/// take a peek.
pub async fn ctrl_c() -> io::Result<()> {
    os_impl::ctrl_c()?.recv().await;
    Ok(())
}
