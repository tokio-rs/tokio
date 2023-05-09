use bytes::buf::Take;
use bytes::Buf;
use core::task::Poll::Ready;
use futures_core::{ready, Future};
use std::io;
use std::io::Read;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Context;
use std::task::Poll;
use tokio::io::AsyncRead;
use tokio::runtime::Handle;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

#[derive(Debug)]
enum State<Buf: bytes::Buf + bytes::BufMut> {
    Idle(Option<Buf>),
    Busy(JoinHandle<(io::Result<usize>, Take<Buf>)>),
}

use State::{Busy, Idle};

/// Use a [`SyncReadIntoAsyncRead`] to asynchronously read from a
/// synchronous API.
#[derive(Debug)]
pub struct SyncReadIntoAsyncRead<R: Read + Send, Buf: bytes::Buf + bytes::BufMut> {
    state: Mutex<State<Buf>>,
    reader: Arc<Mutex<R>>,
    rt: Handle,
}

impl<R: Read + Send, Buf: bytes::Buf + bytes::BufMut> SyncReadIntoAsyncRead<R, Buf> {
    /// This must be called from within a Tokio runtime context, or else it will panic.
    #[track_caller]
    pub fn new(rt: Handle, reader: R) -> Self {
        Self {
            rt,
            state: State::Idle(None).into(),
            reader: Arc::new(reader.into()),
        }
    }

    /// This must be called from within a Tokio runtime context, or else it will panic.
    pub fn new_with_reader(readable: R) -> Self {
        Self::new(Handle::current(), readable)
    }
}

pub(crate) const MAX_BUF: usize = 2 * 1024 * 1024;
/// Repeats operations that are interrupted.
macro_rules! uninterruptibly {
    ($e:expr) => {{
        loop {
            match $e {
                Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
                res => break res,
            }
        }
    }};
}

impl<
        R: Read + Send + 'static + std::marker::Unpin,
        Buf: bytes::Buf + bytes::BufMut + Send + Default + std::marker::Unpin + 'static,
    > AsyncRead for SyncReadIntoAsyncRead<R, Buf>
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        dst: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let me = self.get_mut();
        // Do we need this mutex?
        let state = me.state.get_mut();

        loop {
            match state {
                Idle(ref mut buf_cell) => {
                    let mut buf = buf_cell.take().unwrap_or_default();

                    if buf.has_remaining() {
                        buf.copy_to_slice(dst.initialized_mut());
                        *buf_cell = Some(buf);
                        return Ready(Ok(()));
                    }

                    let target_length = std::cmp::min(dst.remaining(), MAX_BUF);
                    let mut new_buf = vec![0; target_length];

                    let reader = me.reader.clone();
                    *state = Busy(me.rt.spawn_blocking(move || {
                        let result = uninterruptibly!(reader.blocking_lock().read(&mut new_buf));

                        buf.put_slice(&new_buf);

                        if let Ok(n) = result {
                            (result, buf.take(n))
                        } else {
                            (result, buf.take(0))
                        }
                    }));
                }
                Busy(ref mut rx) => {
                    let (result, mut buf) = ready!(Pin::new(rx).poll(cx))?;

                    match result {
                        Ok(_) => {
                            let remaining = buf.remaining();
                            let mut adjusted_dst = dst.take(remaining);
                            buf.copy_to_slice(adjusted_dst.initialize_unfilled());
                            dst.advance(remaining);
                            *state = Idle(None);
                            return Ready(Ok(()));
                        }
                        Err(e) => {
                            *state = Idle(None);
                            return Ready(Err(e));
                        }
                    }
                }
            }
        }
    }
}

impl<R: Read + Send, Buf: bytes::Buf + bytes::BufMut> From<R> for SyncReadIntoAsyncRead<R, Buf> {
    /// This must be called from within a Tokio runtime context, or else it will panic.
    fn from(value: R) -> Self {
        Self::new_with_reader(value)
    }
}
