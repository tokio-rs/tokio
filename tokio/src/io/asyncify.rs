use self::State::*;
use crate::fs::sys;
use crate::io::blocking::Buf;
use crate::io::{AsyncRead, AsyncSeek, AsyncWrite, ReadBuf};
use crate::sync::Mutex;

use std::fmt;
use std::future::Future;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{
    Context,
    Poll::{self, *},
};

/// TODO: docs
pub struct Asyncify<T> {
    pub(crate) io: Arc<T>,
    pub(crate) inner: Mutex<Inner>,
}

pub(crate) struct Inner {
    pub(crate) state: State,

    /// Errors from writes/flushes are returned in write/flush calls. If a write
    /// error is observed while performing a read, it is saved until the next
    /// write / flush call.
    last_write_err: Option<io::ErrorKind>,

    pub(crate) pos: u64,
}

pub(crate) enum State {
    Idle(Option<Buf>),
    Busy(sys::Blocking<(Operation, Buf)>),
}

pub(crate) enum Operation {
    Read(io::Result<usize>),
    Write(io::Result<()>),
    Seek(io::Result<u64>),
}

impl<T> Asyncify<T> {
    /// Creates a new `Asyncify` wrapping some I/O object.
    pub fn new(io: T) -> Self {
        Self {
            io: Arc::new(io),
            inner: Mutex::new(Inner {
                state: State::Idle(Some(Buf::with_capacity(0))),
                last_write_err: None,
                pos: 0,
            }),
        }
    }

    /// Returns a reference to the inner value.
    pub fn get_ref(&self) -> &T {
        &*self.io
    }

    /// Consume `self`, returning the inner I/O object.
    ///
    /// This function is async to allow any in-flight operations to complete.
    ///
    /// Use `Asyncify::try_into_inner` to attempt conversion immediately.
    ///
    /// TODO: example
    pub async fn into_inner(self) -> T {
        self.complete_inflight().await;
        Arc::try_unwrap(self.io).unwrap_or_else(|_| panic!("Arc::try_unwrap failed"))
    }

    /// TODO: docs
    pub fn try_into_inner(mut self) -> Result<T, Self> {
        match Arc::try_unwrap(self.io) {
            Ok(io) => Ok(io),
            Err(io_arc) => {
                self.io = io_arc;
                Err(self)
            }
        }
    }

    // TODO: should this be public?
    /// Wait for any spawned blocking task to complete.
    pub(crate) async fn complete_inflight(&self) {
        let mut inner = self.inner.lock().await;
        inner.complete_inflight().await;
    }

    // TODO: should this be public?
    /// Lift a synchronous I/O operation into a future.
    pub(crate) async fn asyncify<F, Out>(&self, f: F) -> Result<Out, io::Error>
    where
        T: Send + Sync + 'static,
        F: FnOnce(&T) -> Result<Out, io::Error> + Send + 'static,
        Out: Send + 'static,
    {
        let io = self.io.clone();
        crate::fs::asyncify(move || f(&*io)).await
    }
}

impl<T> AsyncRead for Asyncify<T>
where
    // TODO: Requiring bound is odd. Should test it with some more real world use cases. Its
    // necessary because we cannot get a `&mut T` from an `Arc<T>`. Maybe add a mutex?
    for<'a> &'a T: Read,
    T: Send + Sync + 'static,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        dst: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        let inner = me.inner.get_mut();

        loop {
            match inner.state {
                Idle(ref mut buf_cell) => {
                    let mut buf = buf_cell.take().unwrap();

                    if !buf.is_empty() {
                        buf.copy_to(dst);
                        *buf_cell = Some(buf);
                        return Poll::Ready(Ok(()));
                    }

                    buf.ensure_capacity_for(dst);
                    let io = me.io.clone();

                    inner.state = Busy(sys::run(move || {
                        let res = buf.read_from(&mut &*io);
                        (Operation::Read(res), buf)
                    }));
                }
                Busy(ref mut rx) => {
                    let (op, mut buf) = ready!(Pin::new(rx).poll(cx))?;

                    match op {
                        Operation::Read(Ok(_)) => {
                            buf.copy_to(dst);
                            inner.state = Idle(Some(buf));
                            return Ready(Ok(()));
                        }
                        Operation::Read(Err(e)) => {
                            assert!(buf.is_empty());

                            inner.state = Idle(Some(buf));
                            return Ready(Err(e));
                        }
                        Operation::Write(Ok(_)) => {
                            assert!(buf.is_empty());
                            inner.state = Idle(Some(buf));
                            continue;
                        }
                        Operation::Write(Err(e)) => {
                            assert!(inner.last_write_err.is_none());
                            inner.last_write_err = Some(e.kind());
                            inner.state = Idle(Some(buf));
                        }
                        Operation::Seek(result) => {
                            assert!(buf.is_empty());
                            inner.state = Idle(Some(buf));
                            if let Ok(pos) = result {
                                inner.pos = pos;
                            }
                            continue;
                        }
                    }
                }
            }
        }
    }
}

impl<T> AsyncWrite for Asyncify<T>
where
    // TODO: Requiring bound is odd. See `AsyncRead` for more details.
    for<'a> &'a T: Write + Seek,
    T: Send + Sync + 'static,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        src: &[u8],
    ) -> Poll<io::Result<usize>> {
        let me = self.get_mut();
        let inner = me.inner.get_mut();

        if let Some(e) = inner.last_write_err.take() {
            return Ready(Err(e.into()));
        }

        loop {
            match inner.state {
                Idle(ref mut buf_cell) => {
                    let mut buf = buf_cell.take().unwrap();

                    let seek = if !buf.is_empty() {
                        Some(SeekFrom::Current(buf.discard_read()))
                    } else {
                        None
                    };

                    let n = buf.copy_from(src);
                    let io = me.io.clone();

                    inner.state = Busy(sys::run(move || {
                        let res = if let Some(seek) = seek {
                            (&*io).seek(seek).and_then(|_| buf.write_to(&mut &*io))
                        } else {
                            buf.write_to(&mut &*io)
                        };

                        (Operation::Write(res), buf)
                    }));

                    return Ready(Ok(n));
                }
                Busy(ref mut rx) => {
                    let (op, buf) = ready!(Pin::new(rx).poll(cx))?;
                    inner.state = Idle(Some(buf));

                    match op {
                        Operation::Read(_) => {
                            // We don't care about the result here. The fact
                            // that the cursor has advanced will be reflected in
                            // the next iteration of the loop
                            continue;
                        }
                        Operation::Write(res) => {
                            // If the previous write was successful, continue.
                            // Otherwise, error.
                            res?;
                            continue;
                        }
                        Operation::Seek(_) => {
                            // Ignore the seek
                            continue;
                        }
                    }
                }
            }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        let inner = self.inner.get_mut();
        inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.poll_flush(cx)
    }
}

impl<T> AsyncSeek for Asyncify<T>
where
    // TODO: Requiring bound is odd. See `AsyncRead` for more details.
    for<'a> &'a T: Seek,
    T: Send + Sync + 'static,
{
    fn start_seek(self: Pin<&mut Self>, mut pos: SeekFrom) -> io::Result<()> {
        let me = self.get_mut();
        let inner = me.inner.get_mut();

        loop {
            match inner.state {
                Busy(_) => panic!("must wait for poll_complete before calling start_seek"),
                Idle(ref mut buf_cell) => {
                    let mut buf = buf_cell.take().unwrap();

                    // Factor in any unread data from the buf
                    if !buf.is_empty() {
                        let n = buf.discard_read();

                        if let SeekFrom::Current(ref mut offset) = pos {
                            *offset += n;
                        }
                    }

                    let io = me.io.clone();

                    inner.state = Busy(sys::run(move || {
                        let res = (&*io).seek(pos);
                        (Operation::Seek(res), buf)
                    }));
                    return Ok(());
                }
            }
        }
    }

    fn poll_complete(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        let inner = self.inner.get_mut();

        loop {
            match inner.state {
                Idle(_) => return Poll::Ready(Ok(inner.pos)),
                Busy(ref mut rx) => {
                    let (op, buf) = ready!(Pin::new(rx).poll(cx))?;
                    inner.state = Idle(Some(buf));

                    match op {
                        Operation::Read(_) => {}
                        Operation::Write(Err(e)) => {
                            assert!(inner.last_write_err.is_none());
                            inner.last_write_err = Some(e.kind());
                        }
                        Operation::Write(_) => {}
                        Operation::Seek(res) => {
                            if let Ok(pos) = res {
                                inner.pos = pos;
                            }
                            return Ready(res);
                        }
                    }
                }
            }
        }
    }
}

impl Inner {
    pub(crate) async fn complete_inflight(&mut self) {
        use crate::future::poll_fn;

        if let Err(e) = poll_fn(|cx| Pin::new(&mut *self).poll_flush(cx)).await {
            self.last_write_err = Some(e.kind());
        }
    }

    fn poll_flush(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        if let Some(e) = self.last_write_err.take() {
            return Ready(Err(e.into()));
        }

        let (op, buf) = match self.state {
            Idle(_) => return Ready(Ok(())),
            Busy(ref mut rx) => ready!(Pin::new(rx).poll(cx))?,
        };

        // The buffer is not used here
        self.state = Idle(Some(buf));

        match op {
            Operation::Read(_) => Ready(Ok(())),
            Operation::Write(res) => Ready(res),
            Operation::Seek(_) => Ready(Ok(())),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Asyncify<T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Asyncify").field("io", &self.io).finish()
    }
}
