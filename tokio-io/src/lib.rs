#![doc(html_root_url = "https://docs.rs/tokio-io/0.1.12")]
#![deny(missing_debug_implementations, missing_docs, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]
//#![feature(async_await)]

//! Core I/O traits and combinators when working with Tokio.
//!
//! A description of the high-level I/O combinators can be [found online] in
//! addition to a description of the [low level details].
//!
//! [found online]: https://tokio.rs/docs/getting-started/core/
//! [low level details]: https://tokio.rs/docs/going-deeper-tokio/core-low-level/

/// A convenience macro for working with `io::Result<T>` from the `Read` and
/// `Write` traits.
///
/// This macro takes `io::Result<T>` as input, and returns `T` as the output. If
/// the input type is of the `Err` variant, then `Poll::NotReady` is returned if
/// it indicates `WouldBlock` or otherwise `Err` is returned.
#[macro_export]
macro_rules! try_nb {
    ($e:expr) => {
        match $e {
            Ok(t) => t,
            Err(ref e) if e.kind() == ::std::io::ErrorKind::WouldBlock => {
                return ::std::task::Poll::Pending;
            }
            Err(e) => return ::std::task::Poll::Ready(Err(e.into())),
        }
    };
}

macro_rules! ready {
    ($e:expr) => {
        match $e {
            ::std::task::Poll::Ready(t) => t,
            ::std::task::Poll::Pending => return ::std::task::Poll::Pending,
        }
    };
}

pub mod io;

mod allow_std;
mod async_read;
mod async_write;
//mod lines;
//mod split;
mod window;

pub use self::async_read::AsyncRead;
pub use self::async_write::AsyncWrite;

fn _assert_objects() {
    fn _assert<T>() {}
    _assert::<Box<dyn AsyncRead>>();
    _assert::<Box<dyn AsyncWrite>>();
}
