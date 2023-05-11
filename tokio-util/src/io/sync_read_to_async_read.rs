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
    Busy(JoinHandle<(io::Result<usize>, Buf)>),
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
                        // Here, we will split the `buf` into `[..dst.remaining()... ; rest ]`
                        // The `rest` is stuffed into the `buf_cell` for further poll_read.
                        // The other is completely consumed into the unfilled destination.
                        // `rest` can be empty.
                        let mut adjusted_src = buf.copy_to_bytes(
                            std::cmp::min(buf.remaining(), dst.remaining())
                        );
                        let copied_size = adjusted_src.remaining();
                        adjusted_src.copy_to_slice(dst.initialize_unfilled_to(copied_size));
                        dst.set_filled(copied_size);
                        *buf_cell = Some(buf);
                        return Ready(Ok(()));
                    }

                    let reader = me.reader.clone();
                    *state = Busy(me.rt.spawn_blocking(move || {
                        let result = uninterruptibly!(reader.blocking_lock().read(
                                // SAFETY: `reader.read` will *ONLY* write initialized bytes
                                // and never *READ* uninitialized bytes
                                // inside this buffer.
                                //
                                // Furthermore, casting the slice as `*mut [u8]`
                                // is safe because it has the same layout.
                                //
                                // Finally, the pointer obtained is valid and owned
                                // by `buf` only as we have a valid mutable reference
                                // to it, it is valid for write.
                                //
                                // Here, we copy an nightly API: https://doc.rust-lang.org/stable/src/core/mem/maybe_uninit.rs.html#994-998
                                unsafe {
                                        &mut *(buf
                                                .chunk_mut()
                                                .as_uninit_slice_mut()
                                                as *mut [std::mem::MaybeUninit<u8>]
                                                as *mut [u8]
                                            )
                                }
                        ));

                        if let Ok(n) = result {
                            // SAFETY: given we initialize `n` bytes, we can move `n` bytes
                            // forward.
                            unsafe { buf.advance_mut(n); }
                        }

                        (result, buf)
                    }));
                }
                Busy(ref mut rx) => {
                    let (result, mut buf) = ready!(Pin::new(rx).poll(cx))?;

                    match result {
                        Ok(n) => {
                            if n > 0 {
                                let remaining = std::cmp::min(n, dst.remaining());
                                let mut adjusted_src = buf.copy_to_bytes(remaining);
                                adjusted_src.copy_to_slice(dst.initialize_unfilled_to(remaining));
                                dst.advance(remaining);
                            }
                            *state = Idle(Some(buf));
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
