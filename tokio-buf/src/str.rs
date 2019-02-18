use BufStream;
use errors::internal::Never;

use futures::Poll;

use std::io;
use std::mem;

impl BufStream for String {
    type Item = io::Cursor<Vec<u8>>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        let bytes = mem::replace(self, Default::default()).into_bytes();
        let buf = io::Cursor::new(bytes);

        Ok(Some(buf).into())
    }
}

impl BufStream for &'static str {
    type Item = io::Cursor<&'static [u8]>;
    type Error = Never;

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        if self.is_empty() {
            return Ok(None.into());
        }

        let bytes = mem::replace(self, Default::default()).as_bytes();
        let buf = io::Cursor::new(bytes);

        Ok(Some(buf).into())
    }
}
