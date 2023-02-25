#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(tokio_wasi)))] // Wasi does not support panic recovery

use tokio::io::{
    AsyncRead, AsyncWrite, LocklessOwnedReadHalf, LocklessOwnedWriteHalf, LocklessSplit,
    LocklessSplitableOwned, ReadBuf, Shutdown,
};
#[cfg(gat)]
use tokio::io::{LocklessReadHalf, LocklessWriteHalf};

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

#[derive(Debug)]
struct RW;

impl AsyncRead for RW {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        buf.put_slice(&[b'z']);
        Poll::Ready(Ok(()))
    }
}

impl AsyncWrite for RW {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        Poll::Ready(Ok(1))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}

unsafe impl LocklessSplit for RW {}

impl Shutdown for RW {}

#[test]
fn is_send_and_sync() {
    fn assert_bound<T: Send + Sync>() {}

    #[cfg(gat)]
    assert_bound::<LocklessReadHalf<'_, RW>>();
    #[cfg(gat)]
    assert_bound::<LocklessWriteHalf<'_, RW>>();
    assert_bound::<LocklessOwnedReadHalf<RW>>();
    assert_bound::<LocklessOwnedWriteHalf<RW>>();
}

#[test]
fn unsplit_ok() {
    let (r, w) = RW.into_split();
    assert!(r.reunite(w).is_ok());
}

#[test]
#[should_panic]
fn unsplit_err1() {
    let (r, _) = RW.into_split();
    let (_, w) = RW.into_split();
    let _ = r.reunite(w).unwrap();
}

#[test]
#[should_panic]
fn unsplit_err2() {
    let (_, w) = RW.into_split();
    let (r, _) = RW.into_split();
    let _ = r.reunite(w).unwrap();
}
