use bytes::{Bytes, BytesMut};
use futures::Poll;
use never::Never;
use std::io;
use BufStream;

impl BufStream for Vec<u8> {
    type Item = io::Cursor<Vec<u8>>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

impl BufStream for &'static [u8] {
    type Item = io::Cursor<&'static [u8]>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

impl BufStream for Bytes {
    type Item = io::Cursor<Bytes>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

impl BufStream for BytesMut {
    type Item = io::Cursor<BytesMut>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        poll_bytes(self)
    }
}

fn poll_bytes<T: Default>(buf: &mut T) -> Poll<Option<io::Cursor<T>>, Never> {
    use std::mem;

    let bytes = mem::replace(buf, Default::default());
    let buf = io::Cursor::new(bytes);

    Ok(Some(buf).into())
}
