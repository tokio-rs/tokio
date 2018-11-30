use std::io;

use futures::{try_ready, Future, Poll};

use crate::AsyncSeek;

/// A future used to seek an I/O object.
///
/// Created by the [`seek`] function.
///
/// [`seek`]: fn.seek.html
#[derive(Debug)]
pub struct Seek<A> {
    a: Option<A>,
    pos: io::SeekFrom,
}

/// Creates a future which will seek an IO object, and then yield the
/// new position in the object and the object itself.
///
/// In the case of an error, the object will be discarded.
pub fn seek<A>(a: A, pos: io::SeekFrom) -> Seek<A>
    where A: AsyncSeek,
{
    Seek {
        a: Some(a),
        pos,
    }
}

impl<A> Future for Seek<A>
    where A: AsyncSeek,
{
    type Item = (A, u64);
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let pos = try_ready!(
            self.a
                .as_mut()
                .expect("Cannot poll `Seek` after it resolves")
                .poll_seek(self.pos)
        );
        let a = self.a.take().unwrap();
        Ok((a, pos).into())
    }
}
