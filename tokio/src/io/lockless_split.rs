use std::{
    cell::UnsafeCell,
    error::Error,
    fmt::{self, Debug},
    io,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use super::{AsyncRead, AsyncWrite, ReadBuf};

cfg_net_or_io_util! {
    /// Owned Read Half Part
    #[derive(Debug)]
    pub struct LocklessOwnedReadHalf<T>(pub Arc<UnsafeCell<T>>);
    /// Owned Write Half Part
    #[derive(Debug)]
    pub struct LocklessOwnedWriteHalf<T>(pub Arc<UnsafeCell<T>>)
    where
        T: Shutdown;

    unsafe impl<T: Send + Sync> Sync for LocklessOwnedReadHalf<T> {}
    unsafe impl<T: Send + Sync> Send for LocklessOwnedReadHalf<T> {}
    unsafe impl<T: Send + Sync + Shutdown> Sync for LocklessOwnedWriteHalf<T> {}
    unsafe impl<T: Send + Sync + Shutdown> Send for LocklessOwnedWriteHalf<T> {}

    /// Inner split trait
    pub trait LocklessSplitableOwned {
        /// Owned Read Split
        type OwnedReadHalf;
        /// Owned Write Split
        type OwnedWriteHalf;

        /// Split into owned parts
        fn into_split(self) -> (Self::OwnedReadHalf, Self::OwnedWriteHalf);
    }

    impl<T> LocklessSplitableOwned for T
    where
        T: LocklessSplit + Shutdown,
    {
        type OwnedReadHalf = LocklessOwnedReadHalf<T>;

        type OwnedWriteHalf = LocklessOwnedWriteHalf<T>;

        fn into_split(self) -> (Self::OwnedReadHalf, Self::OwnedWriteHalf) {
            let shared = Arc::new(UnsafeCell::new(self));
            (
                LocklessOwnedReadHalf(shared.clone()),
                LocklessOwnedWriteHalf(shared),
            )
        }
    }
}

cfg_net_or_io_util_or_gat! {
    /// Borrowed Write Half Part
    #[derive(Debug)]
    pub struct LocklessWriteHalf<'cx, T>(pub &'cx T);
    /// Borrowed Read Half Part
    #[derive(Debug)]
    pub struct LocklessReadHalf<'cx, T>(pub &'cx T);

    /// The object with has this `LocklessSplit` trait can be safely split
    /// to read/write object in both form of `Owned` or `Borrowed`.
    ///
    /// # Safety
    ///
    /// Users should ensure the read
    /// operations are indenpendent from the write ones, the methods
    /// from `AsyncRead` and `AsyncWrite` can execute concurrently.
    pub unsafe trait LocklessSplit {}

    /// Shutdown trait used for drop OwnedWriteHalf
    pub trait Shutdown {
        /// shutdown write
        fn shutdown(&mut self) {}
    }

    /// Inner split trait
    pub trait LocklessSplitableBorrowed {
        /// Borrowed Read Split
        type Read<'a>
        where
            Self: 'a;
        /// Borrowed Write Split
        type Write<'a>
        where
            Self: 'a;

        /// Split into borrowed parts
        fn split(&mut self) -> (Self::Read<'_>, Self::Write<'_>);
    }

    impl<T> LocklessSplitableBorrowed for T
    where
        T: LocklessSplit + Shutdown,
    {
        type Read<'a> = LocklessReadHalf<'a, T> where Self: 'a;

        type Write<'a> = LocklessWriteHalf<'a, T> where Self: 'a;

        fn split(&mut self) -> (Self::Read<'_>, Self::Write<'_>) {
            (LocklessReadHalf(&*self), LocklessWriteHalf(&*self))
        }
    }

    #[allow(clippy::cast_ref_to_mut)]
    impl<'a, T> AsyncRead for LocklessReadHalf<'a, T>
    where
        T: AsyncRead,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let stream = unsafe { &mut *(self.0 as *const T as *mut T) };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_read(cx, buf)
        }
    }

    #[allow(clippy::cast_ref_to_mut)]
    impl<'a, T> AsyncWrite for LocklessWriteHalf<'a, T>
    where
        T: AsyncWrite,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            let stream = unsafe { &mut *(self.0 as *const T as *mut T) };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_write(cx, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            let stream = unsafe { &mut *(self.0 as *const T as *mut T) };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            let stream = unsafe { &mut *(self.0 as *const T as *mut T) };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_shutdown(cx)
        }
    }
}

cfg_net_or_io_util! {
    impl<T> AsyncRead for LocklessOwnedReadHalf<T>
    where
        T: AsyncRead,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            let stream = unsafe { &mut *self.0.get() };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_read(cx, buf)
        }
    }

    impl<T> AsyncWrite for LocklessOwnedWriteHalf<T>
    where
        T: AsyncWrite + Shutdown,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            let stream = unsafe { &mut *self.0.get() };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_write(cx, buf)
        }

        fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            let stream = unsafe { &mut *self.0.get() };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_flush(cx)
        }

        fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
            let stream = unsafe { &mut *self.0.get() };
            let stream = unsafe { Pin::new_unchecked(stream) };
            stream.poll_shutdown(cx)
        }
    }

    pub(crate) fn reunite<T: Shutdown>(
        read: LocklessOwnedReadHalf<T>,
        write: LocklessOwnedWriteHalf<T>,
    ) -> Result<T, ReuniteError<T>> where T: Debug {
        if Arc::ptr_eq(&read.0, &write.0) {
            // we cannot execute drop for OwnedWriteHalf.
            unsafe {
                let _inner: Arc<UnsafeCell<T>> = std::mem::transmute(write);
            }
            Ok(Arc::try_unwrap(read.0)
                .expect("try_unwrap failed in reunite")
                .into_inner())
        } else {
            Err(ReuniteError(read, write))
        }
    }

    impl<T> LocklessOwnedReadHalf<T>
    where
        T: Shutdown + Debug,
    {
        /// reunite write half
        #[inline]
        pub fn reunite(self, other: LocklessOwnedWriteHalf<T>) -> Result<T, ReuniteError<T>> {
            reunite(self, other)
        }
    }

    impl<T> LocklessOwnedWriteHalf<T>
    where
        T: Shutdown + Debug,
    {
        /// reunite read half
        #[inline]
        pub fn reunite(self, other: LocklessOwnedReadHalf<T>) -> Result<T, ReuniteError<T>> {
            reunite(other, self)
        }
    }

    impl<T> Drop for LocklessOwnedWriteHalf<T>
    where
        T: Shutdown,
    {
        fn drop(&mut self) {
            let stream = unsafe { &mut *self.0.get() };
            stream.shutdown();
        }
    }

    /// Error indicating that two halves were not from the same socket, and thus
    /// could not be reunited.
    #[derive(Debug)]
    pub struct ReuniteError<T: Shutdown>(pub LocklessOwnedReadHalf<T>, pub LocklessOwnedWriteHalf<T>);

    impl<T> fmt::Display for ReuniteError<T>
    where
        T: Shutdown,
    {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "tried to reunite halves")
        }
    }

    impl<T> Error for ReuniteError<T> where T: Shutdown + Debug {}
}
