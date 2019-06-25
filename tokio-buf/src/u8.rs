use crate::never::Never;
use crate::BufStream;
use bytes::{Bytes, BytesMut};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

impl BufStream for Vec<u8> {
    type Item = io::Cursor<Vec<u8>>;
    type Error = Never;

    fn poll_buf(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        if self.is_empty() {
            return Ok(None).into();
        }

        poll_bytes(&mut self)
    }
}

impl BufStream for &'static [u8] {
    type Item = io::Cursor<&'static [u8]>;
    type Error = Never;

    fn poll_buf(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        if self.is_empty() {
            return Ok(None).into();
        }

        poll_bytes(&mut self)
    }
}

impl BufStream for Bytes {
    type Item = io::Cursor<Bytes>;
    type Error = Never;

    fn poll_buf(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        if self.is_empty() {
            return Ok(None).into();
        }

        poll_bytes(&mut self)
    }
}

impl BufStream for BytesMut {
    type Item = io::Cursor<BytesMut>;
    type Error = Never;

    fn poll_buf(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Result<Option<Self::Item>, Self::Error>> {
        if self.is_empty() {
            return Ok(None).into();
        }

        poll_bytes(&mut self)
    }
}

fn poll_bytes<T: Default>(buf: &mut T) -> Poll<Result<Option<io::Cursor<T>>, Never>> {
    use std::mem;

    let bytes = mem::replace(buf, Default::default());
    let buf = io::Cursor::new(bytes);

    Ok(Some(buf)).into()
}
