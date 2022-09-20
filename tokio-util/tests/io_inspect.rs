use std::{
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tokio_util::io::InspectReader;

/// An AsyncRead implementation that works byte-by-byte, to catch out callers
/// who don't allow for `buf` being part-filled before the call
struct SmallReader {
    contents: Vec<u8>,
}

impl Unpin for SmallReader {}

impl AsyncRead for SmallReader {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        if let Some(byte) = self.contents.pop() {
            buf.put_slice(&[byte])
        }
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn read_tee() {
    let contents = b"This could be really long, you know".to_vec();
    let reader = SmallReader {
        contents: contents.clone(),
    };
    let mut altout: Vec<u8> = Vec::new();
    let mut teeout = Vec::new();
    {
        let mut tee = InspectReader::new(reader, |bytes| altout.extend(bytes));
        tee.read_to_end(&mut teeout).await.unwrap();
    }
    assert_eq!(teeout, altout);
    assert_eq!(altout.len(), contents.len());
}
