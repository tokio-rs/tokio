use crate::io::{PollEvented, ReadBuf};

use mio::event::Source;
use std::io;
use std::mem::MaybeUninit;
use std::net::SocketAddr;
use std::task::{ready, Context, Poll};

cfg_io_util! {
    use crate::io::Interest;
    use bytes::BufMut;
}

pub(super) fn sock_addr_to_std(addr: socket2::SockAddr) -> io::Result<SocketAddr> {
    addr.as_socket().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid address family (not IPv4 or IPv6)",
        )
    })
}

/// # Safety
///
/// `op` must not write uninitialized bytes to the passed slice and, if it
/// returns `Ok((n, _))`, must have initialized its first `n` bytes.
unsafe fn read_into_uninit_buf<T>(
    buf: &mut ReadBuf<'_>,
    op: impl FnOnce(&mut [MaybeUninit<u8>]) -> io::Result<(usize, T)>,
) -> io::Result<T> {
    // Safety: `op` reads into `&mut [MaybeUninit<u8>]` and never writes
    // uninitialized bytes to already-initialized portions of `buf`.
    let dst = unsafe { buf.unfilled_mut() };
    let (n, out) = op(dst)?;

    // Safety: Caller guarantees `op` initialized `n` bytes in `dst`.
    unsafe {
        buf.assume_init(n);
    }
    buf.advance(n);
    Ok(out)
}

/// # Safety
///
/// `op` must not write uninitialized bytes to the passed slice and, if it
/// returns `Ok((n, _))`, must have initialized its first `n` bytes.
pub(super) unsafe fn poll_read_uninit<E: Source, T>(
    io: &PollEvented<E>,
    cx: &mut Context<'_>,
    buf: &mut ReadBuf<'_>,
    mut op: impl FnMut(&mut [MaybeUninit<u8>]) -> io::Result<(usize, T)>,
) -> Poll<io::Result<T>> {
    loop {
        let ev = ready!(io.registration().poll_read_ready(cx))?;

        // Safety: Caller guarantees `op` satisfies `read_into_uninit_buf`'s safety contract.
        match unsafe { read_into_uninit_buf(buf, &mut op) } {
            Ok(out) => return Poll::Ready(Ok(out)),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                io.registration().clear_readiness(ev);
            }
            Err(e) => return Poll::Ready(Err(e)),
        }
    }
}

cfg_io_util! {
    /// # Safety
    ///
    /// `op` must not write uninitialized bytes to the passed slice and, if it
    /// returns `Ok((n, _))`, must have initialized its first `n` bytes.
    unsafe fn read_into_buf_mut<B: BufMut, T>(
        buf: &mut B,
        op: impl FnOnce(&mut [MaybeUninit<u8>]) -> io::Result<(usize, T)>,
    ) -> io::Result<T> {
        let dst = buf.chunk_mut();

        // Safety: `chunk_mut()` returns a `&mut UninitSlice`, and `UninitSlice` is a
        // transparent wrapper around `[MaybeUninit<u8>]`. Caller guarantees `op`
        // does not write uninitialized bytes to it.
        let dst = unsafe { dst.as_uninit_slice_mut() };
        let (n, out) = op(dst)?;

        // Safety: Caller guarantees `op` initialized `n` bytes in `dst`.
        unsafe {
            buf.advance_mut(n);
        }

        Ok(out)
    }

    /// # Safety
    ///
    /// `op` must not write uninitialized bytes to the passed slice and, if it
    /// returns `Ok((n, _))`, must have initialized its first `n` bytes.
    pub(super) unsafe fn try_read_buf_uninit<E: Source, B: BufMut, T>(
        io: &PollEvented<E>,
        buf: &mut B,
        op: impl FnOnce(&mut [MaybeUninit<u8>]) -> io::Result<(usize, T)>,
    ) -> io::Result<T> {
        // Safety: Caller guarantees `op` satisfies `read_into_buf_mut`'s safety contract.
        io.registration()
            .try_io(Interest::READABLE, || unsafe { read_into_buf_mut(buf, op) })
    }

    /// # Safety
    ///
    /// `op` must not write uninitialized bytes to the passed slice and, if it
    /// returns `Ok((n, _))`, must have initialized its first `n` bytes.
    pub(super) async unsafe fn async_read_buf_uninit<E: Source, B: BufMut, T>(
        io: &PollEvented<E>,
        interest: Interest,
        buf: &mut B,
        mut op: impl FnMut(&mut [MaybeUninit<u8>]) -> io::Result<(usize, T)>,
    ) -> io::Result<T> {
        io.registration()
            .async_io(interest, || {
                // Safety: Caller guarantees `op` satisfies `read_into_buf_mut`'s safety contract.
                unsafe { read_into_buf_mut(buf, &mut op) }
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::read_into_uninit_buf;
    use crate::io::ReadBuf;
    use std::io;
    use std::mem::MaybeUninit;

    fn copy_partial(dst: &mut [MaybeUninit<u8>], src: &[u8]) -> usize {
        for (slot, byte) in dst.iter_mut().zip(src.iter()) {
            slot.write(*byte);
        }
        src.len().min(dst.len())
    }

    #[test]
    fn read_into_uninit_buf_partial_and_wouldblock() {
        let mut raw = [MaybeUninit::<u8>::uninit(); 16];
        let mut read = ReadBuf::uninit(&mut raw);

        // Safety: `_dst` is untouched on `WouldBlock` error.
        let err = unsafe {
            read_into_uninit_buf::<()>(&mut read, |_dst| Err(io::ErrorKind::WouldBlock.into()))
        }
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
        assert!(read.filled().is_empty());
        assert!(read.initialized().is_empty());

        // Safety: `copy_partial` writes initialized bytes and returns the exact count.
        unsafe { read_into_uninit_buf(&mut read, |dst| Ok((copy_partial(dst, b"abc"), ()))) }
            .unwrap();
        assert_eq!(read.filled(), b"abc");
        assert_eq!(read.initialized(), b"abc");

        // Safety: `copy_partial` writes initialized bytes and returns the exact count.
        unsafe { read_into_uninit_buf(&mut read, |dst| Ok((copy_partial(dst, b"de"), ()))) }
            .unwrap();
        assert_eq!(read.filled(), b"abcde");
        assert_eq!(read.initialized(), b"abcde");
    }

    #[cfg(feature = "io-util")]
    #[test]
    fn read_into_buf_mut_partial_and_wouldblock() {
        use super::read_into_buf_mut;
        use bytes::buf::UninitSlice;
        use bytes::BufMut;

        struct UninitBuf<const N: usize> {
            buf: [MaybeUninit<u8>; N],
            len: usize,
            max_chunk: usize,
            advance_calls: usize,
        }

        impl<const N: usize> UninitBuf<N> {
            fn new(max_chunk: usize) -> Self {
                Self {
                    buf: [MaybeUninit::uninit(); N],
                    len: 0,
                    max_chunk,
                    advance_calls: 0,
                }
            }

            fn initialized(&self) -> &[u8] {
                unsafe { std::slice::from_raw_parts(self.buf.as_ptr().cast(), self.len) }
            }
        }

        unsafe impl<const N: usize> BufMut for UninitBuf<N> {
            fn remaining_mut(&self) -> usize {
                N - self.len
            }

            unsafe fn advance_mut(&mut self, cnt: usize) {
                assert!(cnt <= N - self.len);
                self.len += cnt;
                self.advance_calls += 1;
            }

            fn chunk_mut(&mut self) -> &mut UninitSlice {
                let end = (self.len + self.max_chunk).min(N);
                UninitSlice::uninit(&mut self.buf[self.len..end])
            }
        }

        let mut buf = UninitBuf::<16>::new(4);

        // Safety: `_dst` is untouched on `WouldBlock` error.
        let err = unsafe {
            read_into_buf_mut::<_, usize>(&mut buf, |_dst| Err(io::ErrorKind::WouldBlock.into()))
        }
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::WouldBlock);
        assert_eq!(buf.advance_calls, 0);
        assert_eq!(buf.initialized(), b"");

        // Safety: `copy_partial` writes initialized bytes and returns the exact count.
        let n = unsafe {
            read_into_buf_mut(&mut buf, |dst| {
                let n = copy_partial(dst, b"abc");
                Ok((n, n))
            })
        }
        .unwrap();
        assert_eq!(n, 3);
        assert_eq!(buf.initialized(), b"abc");

        // Safety: `copy_partial` writes initialized bytes and returns the exact count.
        let n = unsafe {
            read_into_buf_mut(&mut buf, |dst| {
                assert_eq!(dst.len(), 4);
                let n = copy_partial(dst, b"defg");
                Ok((n, n))
            })
        }
        .unwrap();
        assert_eq!(n, 4);
        assert_eq!(buf.initialized(), b"abcdefg");
        assert_eq!(buf.advance_calls, 2);
    }
}
