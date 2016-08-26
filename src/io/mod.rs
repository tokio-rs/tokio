//! I/O conveniences when working with primitives in `tokio-core`
//!
//! Contains various combinators to work with I/O objects and type definitions
//! as well.

use std::io;

use futures::BoxFuture;
use futures::stream::BoxStream;

/// A convenience typedef around a `Future` whose error component is `io::Error`
pub type IoFuture<T> = BoxFuture<T, io::Error>;

/// A convenience typedef around a `Stream` whose error component is `io::Error`
pub type IoStream<T> = BoxStream<T, io::Error>;

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
            return ::futures::Poll::NotReady
        }
        Err(e) => return ::futures::Poll::Err(e.into()),
    })
}

mod copy;
mod flush;
mod read_exact;
mod read_to_end;
mod task;
mod window;
mod write_all;
pub use self::copy::{copy, Copy};
pub use self::flush::{flush, Flush};
pub use self::read_exact::{read_exact, ReadExact};
pub use self::read_to_end::{read_to_end, ReadToEnd};
pub use self::task::{TaskIo, TaskIoRead, TaskIoWrite};
pub use self::window::Window;
pub use self::write_all::{write_all, WriteAll};
