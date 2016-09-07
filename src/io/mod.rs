//! I/O conveniences when working with primitives in `tokio-core`
//!
//! Contains various combinators to work with I/O objects and type definitions
//! as well.

use std::io::{self, Read, Write};

use futures::{BoxFuture, Async};
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
            return Ok(::futures::Async::NotReady)
        }
        Err(e) => return Err(e.into()),
    })
}

mod copy;
mod flush;
mod read_exact;
mod read_to_end;
mod split;
mod window;
mod write_all;
pub use self::copy::{copy, Copy};
pub use self::flush::{flush, Flush};
pub use self::read_exact::{read_exact, ReadExact};
pub use self::read_to_end::{read_to_end, ReadToEnd};
pub use self::split::{ReadHalf, WriteHalf};
pub use self::window::Window;
pub use self::write_all::{write_all, WriteAll};

/// A trait for read/write I/O objects
///
/// This trait represents I/O object which are readable and writable.
/// Additionally, they're associated with the ability to test whether they're
/// readable or writable.
///
/// Imporantly, the methods of this trait are intended to be used in conjuction
/// with the current task of a future. Namely whenever any of them return a
/// value that indicates "would block" the current future's task is arranged to
/// receive a notification when the method would otherwise not indicate that it
/// would block.
pub trait Io: Read + Write {
    /// Tests to see if this I/O object may be readable.
    ///
    /// This method returns an `Async<()>` indicating whether the object
    /// **might** be readable. It is possible that even if this method returns
    /// `Async::Ready` that a call to `read` would return a `WouldBlock` error.
    ///
    /// There is a default implementation for this function which always
    /// indicates that an I/O object is readable, but objects which can
    /// implement a finer grained version of this are recommended to do so.
    ///
    /// If this function returns `Async::NotReady` then the current future's
    /// task is arranged to receive a notification when it might not return
    /// `NotReady`.
    ///
    /// # Panics
    ///
    /// This method is likely to panic if called from outside the context of a
    /// future's task.
    fn poll_read(&mut self) -> Async<()> {
        Async::Ready(())
    }

    /// Tests to see if this I/O object may be writable.
    ///
    /// This method returns an `Async<()>` indicating whether the object
    /// **might** be writable. It is possible that even if this method returns
    /// `Async::Ready` that a call to `write` would return a `WouldBlock` error.
    ///
    /// There is a default implementation for this function which always
    /// indicates that an I/O object is writable, but objects which can
    /// implement a finer grained version of this are recommended to do so.
    ///
    /// If this function returns `Async::NotReady` then the current future's
    /// task is arranged to receive a notification when it might not return
    /// `NotReady`.
    ///
    /// # Panics
    ///
    /// This method is likely to panic if called from outside the context of a
    /// future's task.
    fn poll_write(&mut self) -> Async<()> {
        Async::Ready(())
    }

    /// Helper method for splitting this read/write object into two halves.
    ///
    /// The two halves returned implement the `Read` and `Write` traits,
    /// respectively, but are only usable on the current task.
    ///
    /// # Panics
    ///
    /// This method will panic if there is not currently an active future task.
    fn split(self) -> (ReadHalf<Self>, WriteHalf<Self>)
        where Self: Sized
    {
        split::split(self)
    }
}
