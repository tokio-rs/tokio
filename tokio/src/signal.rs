//! Asynchronous signal handling for `tokio`. ctrl-C notifications are
//! supported on both unix and windows systems. For finer grained signal
//! handling support on unix systems, see `tokio_net::signal::unix::Signal`.
pub use tokio_net::signal::ctrl_c;
