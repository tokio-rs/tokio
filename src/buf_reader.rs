#![allow(missing_docs)]

use std::io::{self, Read};
use std::ops::Deref;
use std::sync::Arc;

use ReadinessStream;

use futures::{Task, Poll};
use futures::stream::Stream;

const INPUT_BUF_SIZE: usize = 8 * 1024;

/// A cheap to copy, read-only slice of an input buffer.
#[derive(Clone)]
pub struct InputBuf {
    buf: Arc<Vec<u8>>,
    pos: usize,
    len: usize,
}

impl Deref for InputBuf {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.buf[self.pos..self.pos + self.len]
    }
}

// TODO: implement direct slicing (which clones the Arc)

impl InputBuf {
    fn new() -> InputBuf {
        InputBuf {
            buf: Arc::new(Vec::with_capacity(INPUT_BUF_SIZE)),
            pos: 0,
            len: 0,
        }
    }

    pub fn take(&mut self, len: usize) -> InputBuf {
        assert!(len <= self.len);
        let new = InputBuf {
            buf: self.buf.clone(),
            pos: self.pos,
            len: len,
        };
        self.pos += len;
        self.len -= len;
        new
    }

    pub fn skip(&mut self, len: usize) {
        assert!(len <= self.len);
        self.pos += len;
    }

    fn with_mut<R, F>(&mut self, f: F) -> R
        where F: FnOnce(&mut Vec<u8>) -> R
    {
        // Fast path if we can get mutable access to our own current
        // buffer.
        if let Some(buf) = Arc::get_mut(&mut self.buf) {
            buf.drain(..self.pos);
            self.pos = 0;
            let ret = f(buf);
            self.len = buf.len();
            return ret;
        }

        // If we couldn't get access above then we give ourself a new buffer
        // here.

        let mut v = Vec::with_capacity(INPUT_BUF_SIZE);
        v.extend_from_slice(&self.buf[self.pos..]);
        let ret = f(&mut v);

        self.buf = Arc::new(v);
        self.pos = 0;
        self.len = self.buf.len();
        ret
    }

    fn read<R: Read>(&mut self, socket: &mut R) -> io::Result<(usize, bool)> {
        unsafe fn slice_to_end(v: &mut Vec<u8>) -> &mut [u8] {
            use std::slice;
            if v.capacity() == 0 {
                v.reserve(16);
            }
            if v.capacity() == v.len() {
                v.reserve(1);
            }
            slice::from_raw_parts_mut(v.as_mut_ptr().offset(v.len() as isize),
                                      v.capacity() - v.len())
        }

        self.with_mut(|buf| {
            match socket.read(unsafe { slice_to_end(buf) }) {
                Ok(0) => {
                    trace!("socket EOF");
                    Ok((0, true))
                }
                Ok(n) => {
                    trace!("socket read {} bytes", n);
                    unsafe {
                        let len = buf.len();
                        buf.set_len(len + n);
                    }
                    Ok((n, false))
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok((0, false)),
                Err(e) => Err(e),
            }
        })
    }
}

/// A stream for parsing from an underlying reader, using an unbounded internal
/// buffer.
pub struct BufReader<R> {
    source: R,
    source_ready: ReadinessStream,
    read_ready: bool,
    buf: InputBuf,
}

impl<R: Read + Send + 'static> BufReader<R> {
    pub fn new(source: R, source_ready: ReadinessStream) -> BufReader<R> {
        BufReader {
            source: source,
            source_ready: source_ready,
            read_ready: false,
            buf: InputBuf::new(),
        }
    }

    pub fn buf(&mut self) -> &mut InputBuf {
        &mut self.buf
    }
}

impl<R: Read + Send + 'static> Stream for BufReader<R> {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self, task: &mut Task) -> Poll<Option<()>, io::Error> {
        if !self.read_ready {
            match self.source_ready.poll(task) {
                Poll::NotReady => return Poll::NotReady,
                Poll::Err(e) => return Poll::Err(e.into()),
                Poll::Ok(Some(ref r)) if !r.is_read() => return Poll::NotReady,
                Poll::Ok(Some(_)) => self.read_ready = true,
                _ => unreachable!(),
            }
        }

        match self.buf.read(&mut self.source) {
            Ok((0, true)) => Poll::Ok(None),
            Ok((0, false)) => {
                self.read_ready = false;
                Poll::NotReady
            }
            Ok(_) => {
                self.read_ready = true;
                Poll::Ok(Some(()))
            }
            Err(e) => Poll::Err(e.into()),
        }
    }

    fn schedule(&mut self, task: &mut Task) {
        self.source_ready.schedule(task)
    }
}
