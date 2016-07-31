#![allow(missing_docs)]

use std::io::{self, Write};

use futures::{Future, Task, Poll};
use futures::stream::Stream;
use ReadinessStream;

const OUTPUT_BUF_SIZE: usize = 8 * 1024;

pub struct BufWriter<W> {
    sink: W,
    sink_ready: ReadinessStream,
    write_ready: bool,
    buf: Vec<u8>,
}

impl<W: Write + Send + 'static> BufWriter<W> {
    pub fn new(sink: W, sink_ready: ReadinessStream) -> BufWriter<W> {
        BufWriter {
            sink: sink,
            sink_ready: sink_ready,
            write_ready: false,
            buf: Vec::with_capacity(OUTPUT_BUF_SIZE),
        }
    }

    pub fn extend(&mut self, data: &[u8]) {
        extend(&mut self.buf, data)
    }

    pub fn flush(self) -> Flush<W> {
        Flush { writer: Some(self) }
    }

    pub fn reserve(self, amt: usize) -> Reserve<W> {
        Reserve { amt: amt, writer: Some(self) }
    }

    /// Is there buffered data waiting to be sent?
    pub fn is_dirty(&self) -> bool {
        self.buf.len() > 0
    }

    fn poll_flush(&mut self, task: &mut Task) -> Poll<(), io::Error> {
        let mut task = task.scoped();
        while self.is_dirty() {
            if !self.write_ready {
                match self.sink_ready.poll(&mut task) {
                    Poll::Err(e) => return Poll::Err(e),
                    Poll::Ok(Some(ref r)) if !r.is_write() => return Poll::NotReady,
                    Poll::Ok(Some(_)) => self.write_ready = true,
                    Poll::Ok(None) | // TODO: this should translate to an error
                    Poll::NotReady => return Poll::NotReady,
                }
            }

            debug!("trying to write some data");
            match self.sink.write(&self.buf) {
                Ok(0) => return Poll::Err(io::Error::new(io::ErrorKind::Other, "early eof")),
                Ok(n) => {
                    // TODO: consider draining more lazily, i.e. only just
                    // before returning
                    self.buf.drain(..n);
                }
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.write_ready = false;
                }
                Err(e) => return Poll::Err(e),
            }

            task.ready();
        }

        debug!("fully flushed");
        Poll::Ok(())
    }
}

impl<W: Write> Write for BufWriter<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        extend(&mut self.buf, data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // TODO: something reasonable
        unimplemented!()
    }
}

pub struct Flush<W> {
    writer: Option<BufWriter<W>>,
}

impl<W: Write + Send + 'static> Flush<W> {
    pub fn is_dirty(&self) -> bool {
        self.writer.as_ref().unwrap().is_dirty()
    }

    pub fn into_inner(mut self) -> BufWriter<W> {
        self.writer.take().unwrap()
    }
}

impl<W: Write + Send + 'static> Future for Flush<W> {
    type Item = BufWriter<W>;
    type Error = (io::Error, BufWriter<W>);

    fn poll(&mut self, task: &mut Task)
            -> Poll<BufWriter<W>, (io::Error, BufWriter<W>)> {
        match self.writer.as_mut().unwrap().poll_flush(task) {
            Poll::Ok(()) => Poll::Ok(self.writer.take().unwrap()),
            Poll::Err(e) => Poll::Err((e, self.writer.take().unwrap())),
            Poll::NotReady => Poll::NotReady,
        }
    }

    fn schedule(&mut self, task: &mut Task) {
        let writer = self.writer.as_mut().unwrap();

        assert!(!writer.write_ready);
        writer.sink_ready.schedule(task)
    }
}

// TODO: why doesn't extend_from_slice optimize to this?
fn extend(dst: &mut Vec<u8>, data: &[u8]) {
    use std::ptr;
    dst.reserve(data.len());
    let prev = dst.len();
    unsafe {
        ptr::copy_nonoverlapping(data.as_ptr(),
                                 dst.as_mut_ptr().offset(prev as isize),
                                 data.len());
        dst.set_len(prev + data.len());
    }
}

pub struct Reserve<W> {
    amt: usize,
    writer: Option<BufWriter<W>>,
}

impl<W: Write + Send + 'static> Future for Reserve<W> {
    type Item = BufWriter<W>;
    type Error = (io::Error, BufWriter<W>);

    fn poll(&mut self, task: &mut Task)
            -> Poll<BufWriter<W>, (io::Error, BufWriter<W>)> {
        loop {
            let (cap, len) = {
                let buf = &mut self.writer.as_mut().unwrap().buf;
                (buf.capacity(), buf.len())
            };

            if self.amt <= cap - len {
                return Poll::Ok(self.writer.take().unwrap())
            } else if self.amt > cap {
                let mut writer = self.writer.take().unwrap();
                writer.buf.reserve(self.amt);
                return Poll::Ok(writer)
            }

            match self.writer.as_mut().unwrap().poll_flush(task) {
                Poll::Ok(()) => {},
                Poll::Err(e) => return Poll::Err((e, self.writer.take().unwrap())),
                Poll::NotReady => return Poll::NotReady,
            }
        }
    }

    fn schedule(&mut self, task: &mut Task) {
        let writer = self.writer.as_mut().unwrap();

        assert!(!writer.write_ready);
        writer.sink_ready.schedule(task)
    }
}
