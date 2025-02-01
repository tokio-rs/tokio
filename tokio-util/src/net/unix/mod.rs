//! Unix domain socket helpers.
use super::Listener;
use std::io::Result;
use std::task::{Context, Poll};

impl Listener for tokio::net::UnixListener {
    type Io = tokio::net::UnixStream;
    type Addr = tokio::net::unix::SocketAddr;

    fn poll_accept(&mut self, cx: &mut Context<'_>) -> Poll<Result<(Self::Io, Self::Addr)>> {
        // Call the concrete `poll_accept` method of `tokio::net::UnixListener`
        <tokio::net::UnixListener>::poll_accept(self, cx)
    }

    fn local_addr(&self) -> Result<Self::Addr> {
        // Return the result of `local_addr` directly
        self.local_addr()
    }
}
