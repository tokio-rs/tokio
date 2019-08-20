use std::task::{Context, Poll};
use std::io;

pub trait AsyncDatagram {
    type Address: Unpin;

    fn poll_datagram_recv(&self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<Result<(usize, Self::Address), io::Error>>;
    fn poll_datagram_send(&self, cx: &mut Context<'_>, buf: &mut [u8], target: &Self::Address) -> Poll<io::Result<usize>>;
}
