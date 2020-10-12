#![doc(html_root_url = "https://docs.rs/tokio-util/0.4.0")]
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

/*
Disabled due to removal of poll_ functions on UdpSocket.

See https://github.com/tokio-rs/tokio/issues/2830
cfg_udp! {
    pub mod udp;
}
*/

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
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub(crate) fn poll_read_buf<T: AsyncRead>(
        cx: &mut Context<'_>,
        io: Pin<&mut T>,
        buf: &mut impl BufMut,
    ) -> Poll<io::Result<usize>> {
        if !buf.has_remaining_mut() {
            return Poll::Ready(Ok(0));
        }

        let orig = buf.bytes_mut().as_ptr() as *const u8;
        let mut b = ReadBuf::uninit(buf.bytes_mut());

        ready!(io.poll_read(cx, &mut b))?;
        let n = b.filled().len();

        // Safety: we can assume `n` bytes were read, since they are in`filled`.
        assert_eq!(orig, b.filled().as_ptr());
        unsafe {
            buf.advance_mut(n);
        }
        Poll::Ready(Ok(n))
    }
}
