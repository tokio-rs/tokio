#![cfg(feature = "full")]

use tokio::io::{AsyncBufReadExt, AsyncReadExt};

#[tokio::test]
#[cfg_attr(miri, ignore)] // Miri doesn't support write to event (inside mio::waker::Waker::wake)
async fn empty_read_is_cooperative() {
    tokio::select! {
        biased;

        _ = async {
            loop {
                let mut buf = [0u8; 4096];
                let _ = tokio::io::empty().read(&mut buf).await;
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

#[tokio::test]
#[cfg_attr(miri, ignore)] // Miri doesn't support write to event (inside mio::waker::Waker::wake)
async fn empty_buf_reads_are_cooperative() {
    tokio::select! {
        biased;

        _ = async {
            loop {
                let mut buf = String::new();
                let _ = tokio::io::empty().read_line(&mut buf).await;
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}
