use never::Never;
use BufStream;
use SizeHint;

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

    fn size_hint(&self) -> SizeHint {
        size_hint(&self[..])
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

    fn size_hint(&self) -> SizeHint {
        size_hint(&self[..])
    }
}

fn size_hint(s: &str) -> SizeHint {
    let mut hint = SizeHint::new();
    hint.set_lower(s.len() as u64);
    hint.set_upper(s.len() as u64);
    hint
}
