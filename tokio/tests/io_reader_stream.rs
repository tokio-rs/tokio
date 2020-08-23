#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncRead;
use tokio::stream::StreamExt;

/// produces at most `remaining` zeros, that returns error.
/// each time it reads at most 31 byte.
struct Reader {
    remaining: usize,
}

impl AsyncRead for Reader {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = Pin::into_inner(self);
        assert_ne!(buf.len(), 0);
        if this.remaining > 0 {
            let n = std::cmp::min(this.remaining, buf.len());
            let n = std::cmp::min(n, 31);
            for x in &mut buf[..n] {
                *x = 0;
            }
            this.remaining -= n;
            Poll::Ready(Ok(n))
        } else {
            Poll::Ready(Err(std::io::Error::from_raw_os_error(22)))
        }
    }
}

#[tokio::test]
async fn correct_behavior_on_errors() {
    let reader = Reader { remaining: 8000 };
    let mut stream = tokio::io::reader_stream(reader);
    let mut zeros_received = 0;
    let mut had_error = false;
    loop {
        let item = stream.next().await.unwrap();
        match item {
            Ok(bytes) => {
                let bytes = &*bytes;
                for byte in bytes {
                    assert_eq!(*byte, 0);
                    zeros_received += 1;
                }
            }
            Err(_) => {
                assert!(!had_error);
                had_error = true;
                break;
            }
        }
    }

    assert!(had_error);
    assert_eq!(zeros_received, 8000);
    assert!(stream.next().await.is_none());
}
