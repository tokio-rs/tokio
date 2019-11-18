#[cfg(unix)]
use super::unix::{self as os_impl};
#[cfg(windows)]
use super::windows::{self as os_impl};

use std::io;

/// Completes when a "ctrl-c" notification is sent to the process.
///
/// While signals are handled very differently between Unix and Windows, both
/// platforms support receiving a signal on "ctrl-c". This function provides a
/// portable API for receiving this notification.
///
/// Once the returned future is polled, a listener a listener is registered. The
/// future will complete on the first received `ctrl-c` **after** the initial
/// call to either `Future::poll` or `.await`.
///
/// # Examples
///
/// ```rust,no_run
/// use tokio::signal;
///
/// #[tokio::main]
/// async fn main() {
///     println!("waiting for ctrl-c");
///
///     signal::ctrl_c().await.expect("failed to listen for event");
///
///     println!("received ctrl-c event");
/// }
/// ```
pub async fn ctrl_c() -> io::Result<()> {
    os_impl::ctrl_c()?.recv().await;
    Ok(())
}
