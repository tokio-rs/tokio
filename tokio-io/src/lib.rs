#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-io/0.1.8")]

//! Core I/O traits and combinators when working with Tokio.
//!
//! A description of the high-level I/O combinators can be [found online] in
//! addition to a description of the [low level details].
//!
//! [found online]: https://tokio.rs/docs/getting-started/core/
//! [low level details]: https://tokio.rs/docs/going-deeper-tokio/core-low-level/

#[macro_use]
extern crate log;

#[macro_use]
extern crate futures;
extern crate bytes;

use std::io as std_io;

use futures::{Future, Stream};

/// A convenience typedef around a `Future` whose error component is `io::Error`
pub type IoFuture<T> = Box<Future<Item = T, Error = std_io::Error> + Send>;

/// A convenience typedef around a `Stream` whose error component is `io::Error`
pub type IoStream<T> = Box<Stream<Item = T, Error = std_io::Error> + Send>;

/// A convenience macro for working with `io::Result<T>` from the `Read` and
/// `Write` traits.
///
/// This macro takes `io::Result<T>` as input, and returns `T` as the output. If
/// the input type is of the `Err` variant, then `Poll::NotReady` is returned if
/// it indicates `WouldBlock` or otherwise `Err` is returned.
#[macro_export]
macro_rules! try_nb {
    ($e:expr) => (match $e {
        Ok(t) => t,
        Err(ref e) if e.kind() == ::std::io::ErrorKind::WouldBlock => {
            return Ok(::futures::Async::NotReady)
        }
        Err(e) => return Err(e.into()),
    })
}

pub mod io;
pub mod codec;

mod allow_std;
mod async_read;
mod async_write;
mod framed;
mod framed_read;
mod framed_write;
mod length_delimited;
mod lines;
mod split;
mod window;
pub mod _tokio_codec;

pub use self::async_read::AsyncRead;
pub use self::async_write::AsyncWrite;

fn _assert_objects() {
    fn _assert<T>() {}
    _assert::<Box<AsyncRead>>();
    _assert::<Box<AsyncWrite>>();
}
