#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Utilities for working with Tokio.
//!
//! This crate is not versioned in lockstep with the core
//! [`tokio`] crate. However, `tokio-util` _will_ respect Rust's
//! semantic versioning policy, especially with regard to breaking changes.
//!
//! [`tokio`]: https://docs.rs/tokio

#[macro_use]
mod cfg;

mod loom;

cfg_codec! {
    pub mod codec;
}

cfg_net! {
    #[cfg(not(target_arch = "wasm32"))]
    pub mod udp;
    pub mod net;
}

cfg_compat! {
    pub mod compat;
}

cfg_io! {
    pub mod io;
}

cfg_rt! {
    pub mod context;
    pub mod task;
}

cfg_time! {
    pub mod time;
}

pub mod sync;

pub mod either;

pub use bytes;

mod util {
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

    use bytes::{Buf, BufMut};
    use futures_core::ready;
    use std::future::Future;
    use std::io::{self, IoSlice};
    use std::mem::MaybeUninit;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Try to read data from an `AsyncRead` into an implementer of the [`BufMut`] trait.
    ///
    /// [`BufMut`]: bytes::Buf
    ///
    /// # Example
    ///
    /// ```
    /// use bytes::{Bytes, BytesMut};
    /// use tokio_stream as stream;
    /// use tokio::io::Result;
    /// use tokio_util::io::{StreamReader, poll_read_buf};
    /// use futures::future::poll_fn;
    /// use std::pin::Pin;
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///
    /// // Create a reader from an iterator. This particular reader will always be
    /// // ready.
    /// let mut read = StreamReader::new(stream::iter(vec![Result::Ok(Bytes::from_static(&[0, 1, 2, 3]))]));
    ///
    /// let mut buf = BytesMut::new();
    /// let mut reads = 0;
    ///
    /// loop {
    ///     reads += 1;
    ///     let n = poll_fn(|cx| poll_read_buf(Pin::new(&mut read), cx, &mut buf)).await?;
    ///
    ///     if n == 0 {
    ///         break;
    ///     }
    /// }
    ///
    /// // one or more reads might be necessary.
    /// assert!(reads >= 1);
    /// assert_eq!(&buf[..], &[0, 1, 2, 3]);
    /// # Ok(())
    /// # }
    /// ```
    #[cfg(any(feature = "io", feature = "coded"))]
    #[cfg_attr(not(feature = "io"), allow(unreachable_pub))]
    pub fn poll_read_buf<T: AsyncRead, B: BufMut>(
        io: Pin<&mut T>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        if !buf.has_remaining_mut() {
            return Poll::Ready(Ok(0));
        }

        let n = {
            let dst = buf.chunk_mut();

            // Safety: `chunk_mut()` returns a `&mut UninitSlice`, and `UninitSlice` is a
            // transparent wrapper around `[MaybeUninit<u8>]`.
            let dst = unsafe { &mut *(dst as *mut _ as *mut [MaybeUninit<u8>]) };
            let mut buf = ReadBuf::uninit(dst);
            let ptr = buf.filled().as_ptr();
            ready!(io.poll_read(cx, &mut buf)?);

            // Ensure the pointer does not change from under us
            assert_eq!(ptr, buf.filled().as_ptr());
            buf.filled().len()
        };

        // Safety: This is guaranteed to be the number of initialized (and read)
        // bytes due to the invariants provided by `ReadBuf::filled`.
        unsafe {
            buf.advance_mut(n);
        }

        Poll::Ready(Ok(n))
    }

    /// Try to write data from an implementer of the [`Buf`] trait to an
    /// [`AsyncWrite`], advancing the buffer's internal cursor.
    ///
    /// This function will use [vectored writes] when the [`AsyncWrite`] supports
    /// vectored writes.
    ///
    /// # Examples
    ///
    /// [`File`] implements [`AsyncWrite`] and [`Cursor<&[u8]>`] implements
    /// [`Buf`]:
    ///
    /// ```no_run
    /// use tokio_util::io::poll_write_buf;
    /// use tokio::io;
    /// use tokio::fs::File;
    ///
    /// use bytes::Buf;
    /// use std::io::Cursor;
    /// use std::pin::Pin;
    /// use futures::future::poll_fn;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut file = File::create("foo.txt").await?;
    ///     let mut buf = Cursor::new(b"data to write");
    ///
    ///     // Loop until the entire contents of the buffer are written to
    ///     // the file.
    ///     while buf.has_remaining() {
    ///         poll_fn(|cx| poll_write_buf(Pin::new(&mut file), cx, &mut buf)).await?;
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    ///
    /// [`Buf`]: bytes::Buf
    /// [`AsyncWrite`]: tokio::io::AsyncWrite
    /// [`File`]: tokio::fs::File
    /// [vectored writes]: tokio::io::AsyncWrite::poll_write_vectored
    #[cfg(any(feature = "io", feature = "coded"))]
    #[cfg_attr(not(feature = "io"), allow(unreachable_pub))]
    pub fn poll_write_buf<T: AsyncWrite, B: Buf>(
        io: Pin<&mut T>,
        cx: &mut Context<'_>,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        const MAX_BUFS: usize = 64;

        if !buf.has_remaining() {
            return Poll::Ready(Ok(0));
        }

        let n = if io.is_write_vectored() {
            let mut slices = [IoSlice::new(&[]); MAX_BUFS];
            let cnt = buf.chunks_vectored(&mut slices);
            ready!(io.poll_write_vectored(cx, &slices[..cnt]))?
        } else {
            ready!(io.poll_write(cx, buf.chunk()))?
        };

        buf.advance(n);

        Poll::Ready(Ok(n))
    }

    /// A wrapper type that tells the compiler that the contents might not be valid.
    ///
    /// This is necessary mainly when `T` contains a reference. In that case, the
    /// compiler will sometimes assume that the reference is always valid; in some
    /// cases it will assume this even after the destructor of `T` runs. For
    /// example, when a reference is used as a function argument, then the compiler
    /// will assume that the reference is valid until the function returns, even if
    /// the reference is destroyed during the function. When the reference is used
    /// as part of a self-referential struct, that assumption can be false. Wrapping
    /// the reference in this type prevents the compiler from making that
    /// assumption.
    ///
    /// # Invariants
    ///
    /// The `MaybeUninit` will always contain a valid value until the destructor runs.
    //
    // Reference
    // See <https://users.rust-lang.org/t/unsafe-code-review-semi-owning-weak-rwlock-t-guard/95706>
    //
    // TODO: replace this with an official solution once RFC #3336 or similar is available.
    // <https://github.com/rust-lang/rfcs/pull/3336>
    #[repr(transparent)]
    pub(crate) struct MaybeDangling<T>(MaybeUninit<T>);

    impl<T> Drop for MaybeDangling<T> {
        fn drop(&mut self) {
            // Safety: `0` is always initialized.
            unsafe { core::ptr::drop_in_place(self.0.as_mut_ptr()) };
        }
    }

    impl<T> MaybeDangling<T> {
        pub(crate) fn new(inner: T) -> Self {
            Self(MaybeUninit::new(inner))
        }
    }

    impl<F: Future> Future for MaybeDangling<F> {
        type Output = F::Output;

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            // Safety: `0` is always initialized.
            let fut = unsafe { self.map_unchecked_mut(|this| this.0.assume_init_mut()) };
            fut.poll(cx)
        }
    }

    #[test]
    fn maybedangling_runs_drop() {
        struct SetOnDrop<'a>(&'a mut bool);

        impl Drop for SetOnDrop<'_> {
            fn drop(&mut self) {
                *self.0 = true;
            }
        }

        let mut success = false;

        drop(MaybeDangling::new(SetOnDrop(&mut success)));
        assert!(success);
    }
}
