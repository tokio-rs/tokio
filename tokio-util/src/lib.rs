#![doc(html_root_url = "https://docs.rs/tokio-util/0.5.0")]
#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![cfg_attr(docsrs, deny(broken_intra_doc_links))]
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
    pub mod udp;
}

cfg_compat! {
    pub mod compat;
}

cfg_io! {
    pub mod io;
}

cfg_rt! {
    pub mod context;
}

pub mod sync;

pub mod either;

#[cfg(feature = "time")]
pub mod time;

#[cfg(any(feature = "io", feature = "codec"))]
mod util {
    use tokio::io::{AsyncRead, ReadBuf};

    use bytes::BufMut;
    use futures_core::ready;
    use std::io;
    use std::mem::MaybeUninit;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    /// Try to read data from an `AsyncRead` into an implementer of the [`Buf`] trait.
    ///
    /// [`Buf`]: bytes::Buf
    ///
    /// # Example
    ///
    /// ```
    /// use bytes::{Bytes, BytesMut};
    /// use tokio::stream;
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
            let dst = buf.bytes_mut();
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
}
