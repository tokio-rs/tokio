//! I/O conveniences when working with primitives in `tokio-core`
//!
//! Contains various combinators to work with I/O objects and type definitions
//! as well.

use std::io::{self, Read, Write};

use futures::{BoxFuture, Async, Poll};
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
mod read;
mod read_until;
mod split;
mod window;
mod write_all;
pub use self::copy::{copy, Copy};
pub use self::flush::{flush, Flush};
pub use self::read_exact::{read_exact, ReadExact};
pub use self::read_to_end::{read_to_end, ReadToEnd};
pub use self::read::read;
pub use self::read_until::{read_until, ReadUntil};
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

/// A trait for framed reading and writing.
///
/// Most implementations of `FramedIo` are for doing protocol level
/// serialization and deserialization.
///
/// Imporantly, the methods of this trait are intended to be used in conjuction
/// with the current task of a future. Namely whenever any of them return a
/// value that indicates "would block" the current future's task is arranged to
/// receive a notification when the method would otherwise not indicate that it
/// would block.
pub trait FramedIo {
    /// Messages written
    type In;

    /// Messages read
    type Out;

    /// Tests to see if this `FramedIo` may be readable.
    fn poll_read(&mut self) -> Async<()>;

    /// Read a message frame from the `FramedIo`
    fn read(&mut self) -> Poll<Self::Out, io::Error>;

    /// Tests to see if this `FramedIo` may be writable.
    ///
    /// Unlike most other calls to poll readiness, it is important that when
    /// `FramedIo::poll_write` returns `Async::Ready` that a write will
    /// succeed.
    fn poll_write(&mut self) -> Async<()>;

    /// Write a message frame to the `FramedIo`
    fn write(&mut self, req: Self::In) -> Poll<(), io::Error>;

    /// Flush pending writes or do any other work not driven by reading /
    /// writing.
    ///
    /// Since the backing source is non-blocking, there is no guarantee that a
    /// call to `FramedIo::write` is able to write the full message to the
    /// backing source immediately. In this case, the `FramedIo` will need to
    /// buffer the remaining data to write. Calls to `FramedIo:flush` attempt
    /// to write any remaining data in the write buffer to the underlying
    /// source.
    fn flush(&mut self) -> Poll<(), io::Error>;
}
