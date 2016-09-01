use std::io::{self, Read, Write};

use futures::{Future, Poll};

/// A future which will copy all data from a reader into a writer.
///
/// Created by the `copy` function, this future will resolve to the number of
/// bytes copied or an error if one happens.
pub struct Copy<R, W> {
    reader: R,
    read_done: bool,
    writer: W,
    pos: usize,
    cap: usize,
    amt: u64,
    buf: Box<[u8]>,
}

/// Creates a future which represents copying all the bytes from one object to
/// another.
///
/// The returned future will copy all the bytes read from `reader` into the
/// `writer` specified. This future will only complete once the `reader` has hit
/// EOF and all bytes have been written to and flushed from the `writer`
/// provided.
///
/// On success the number of bytes is returned and the `reader` and `writer` are
/// consumed. On error the error is returned and the I/O objects are consumed as
/// well.
pub fn copy<R, W>(reader: R, writer: W) -> Copy<R, W>
    where R: Read,
          W: Write,
{
    Copy {
        reader: reader,
        read_done: false,
        writer: writer,
        amt: 0,
        pos: 0,
        cap: 0,
        buf: Box::new([0; 2048]),
    }
}

impl<R, W> Future for Copy<R, W>
    where R: Read,
          W: Write,
{
    type Item = u64;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<u64, io::Error> {
        loop {
            // If our buffer is empty, then we need to read some data to
            // continue.
            if self.pos == self.cap && !self.read_done {
                let n = try_nb!(self.reader.read(&mut self.buf));
                if n == 0 {
                    self.read_done = true;
                } else {
                    self.pos = 0;
                    self.cap = n;
                }
            }

            // If our buffer has some data, let's write it out!
            while self.pos < self.cap {
                let i = try_nb!(self.writer.write(&self.buf[self.pos..self.cap]));
                self.pos += i;
                self.amt += i as u64;
            }

            // If we've written al the data and we've seen EOF, flush out the
            // data and finish the transfer.
            // done with the entire transfer.
            if self.pos == self.cap && self.read_done {
                try_nb!(self.writer.flush());
                return Ok(self.amt.into())
            }
        }
    }
}
