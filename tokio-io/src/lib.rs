//! Core I/O traits and combinators when working with Tokio.
//!
//! A description of the high-level I/O combinators can be [found online] in
//! addition to a description of the [low level details].
//!
//! [found online]: https://tokio.rs/docs/getting-started/core/
//! [low level details]: https://tokio.rs/docs/going-deeper-tokio/core-low-level/

#![deny(missing_docs, missing_debug_implementations, warnings)]
#![doc(html_root_url = "https://docs.rs/tokio-io/0.1")]

#[macro_use]
extern crate log;

#[macro_use]
extern crate futures;
extern crate bytes;

use std::io as std_io;
use std::io::Write;

use futures::{Async, Future, Poll, Stream};
use bytes::{Buf, BufMut};

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
mod copy;
mod flush;
mod framed;
mod framed_read;
mod framed_write;
mod length_delimited;
mod lines;
mod read;
mod read_exact;
mod read_to_end;
mod read_until;
mod shutdown;
mod split;
mod window;
mod write_all;

use codec::{Decoder, Encoder, Framed};
use split::{ReadHalf, WriteHalf};

/// A trait for readable objects which operated in an asynchronous and
/// futures-aware fashion.
///
/// This trait inherits from `io::Read` and indicates as a marker that an I/O
/// object is **nonblocking**, meaning that it will return an error instead of
/// blocking when bytes are unavailable, but the stream hasn't reached EOF.
/// Specifically this means that the `read` function for types that implement
/// this trait can have a few return values:
///
/// * `Ok(n)` means that `n` bytes of data was immediately read and placed into
///   the output buffer, where `n` == 0 implies that EOF has been reached.
/// * `Err(e) if e.kind() == ErrorKind::WouldBlock` means that no data was read
///   into the buffer provided. The I/O object is not currently readable but may
///   become readable in the future. Most importantly, **the current future's
///   task is scheduled to get unparked when the object is readable**. This
///   means that like `Future::poll` you'll receive a notification when the I/O
///   object is readable again.
/// * `Err(e)` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the `read` method only works in the
/// context of a future's task. The object may panic if used outside of a task.
pub trait AsyncRead: std_io::Read {
    /// Prepares an uninitialized buffer to be safe to pass to `read`. Returns
    /// `true` if the supplied buffer was zeroed out.
    ///
    /// While it would be highly unusual, implementations of [`io::Read`] are
    /// able to read data from the buffer passed as an argument. Because of
    /// this, the buffer passed to [`io::Read`] must be initialized memory. In
    /// situations where large numbers of buffers are used, constantly having to
    /// zero out buffers can be expensive.
    ///
    /// This function does any necessary work to prepare an uninitialized buffer
    /// to be safe to pass to `read`. If `read` guarantees to never attempt read
    /// data out of the supplied buffer, then `prepare_uninitialized_buffer`
    /// doesn't need to do any work.
    ///
    /// If this function returns `true`, then the memory has been zeroed out.
    /// This allows implementations of `AsyncRead` which are composed of
    /// multiple sub implementations to efficiently implement
    /// `prepare_uninitialized_buffer`.
    ///
    /// This function isn't actually `unsafe` to call but `unsafe` to implement.
    /// The implementor must ensure that either the whole `buf` has been zeroed
    /// or `read_buf()` overwrites the buffer without reading it and returns
    /// correct value.
    ///
    /// This function is called from [`read_buf`].
    ///
    /// [`io::Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
    /// [`read_buf`]: #method.read_buf
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        for i in 0..buf.len() {
            buf[i] = 0;
        }

        true
    }

    /// Pull some bytes from this source into the specified `Buf`, returning
    /// how many bytes were read.
    ///
    /// The `buf` provided will have bytes read into it and the internal cursor
    /// will be advanced if any bytes were read. Note that this method typically
    /// will not reallocate the buffer provided.
    fn read_buf<B: BufMut>(&mut self, buf: &mut B) -> Poll<usize, std_io::Error>
        where Self: Sized,
    {
        if !buf.has_remaining_mut() {
            return Ok(Async::Ready(0));
        }

        unsafe {
            let n = {
                let b = buf.bytes_mut();

                self.prepare_uninitialized_buffer(b);

                try_nb!(self.read(b))
            };

            buf.advance_mut(n);
            Ok(Async::Ready(n))
        }
    }

    /// Provides a `Stream` and `Sink` interface for reading and writing to this
    /// `Io` object, using `Decode` and `Encode` to read and write the raw data.
    ///
    /// Raw I/O objects work with byte sequences, but higher-level code usually
    /// wants to batch these into meaningful chunks, called "frames". This
    /// method layers framing on top of an I/O object, by using the `Codec`
    /// traits to handle encoding and decoding of messages frames. Note that
    /// the incoming and outgoing frame types may be distinct.
    ///
    /// This function returns a *single* object that is both `Stream` and
    /// `Sink`; grouping this into a single object is often useful for layering
    /// things like gzip or TLS, which require both read and write access to the
    /// underlying object.
    ///
    /// If you want to work more directly with the streams and sink, consider
    /// calling `split` on the `Framed` returned by this method, which will
    /// break them into separate objects, allowing them to interact more easily.
    fn framed<T: Encoder + Decoder>(self, codec: T) -> Framed<Self, T>
        where Self: AsyncWrite + Sized,
    {
        framed::framed(self, codec)
    }

    /// Helper method for splitting this read/write object into two halves.
    ///
    /// The two halves returned implement the `Read` and `Write` traits,
    /// respectively.
    fn split(self) -> (ReadHalf<Self>, WriteHalf<Self>)
        where Self: AsyncWrite + Sized,
    {
        split::split(self)
    }
}

impl<T: ?Sized + AsyncRead> AsyncRead for Box<T> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        (**self).prepare_uninitialized_buffer(buf)
    }
}

impl<'a, T: ?Sized + AsyncRead> AsyncRead for &'a mut T {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        (**self).prepare_uninitialized_buffer(buf)
    }
}

impl<'a> AsyncRead for &'a [u8] {
    unsafe fn prepare_uninitialized_buffer(&self, _buf: &mut [u8]) -> bool {
        false
    }
}

/// A trait for writable objects which operated in an asynchronous and
/// futures-aware fashion.
///
/// This trait inherits from `io::Write` and indicates that an I/O object is
/// **nonblocking**, meaning that it will return an error instead of blocking
/// when bytes cannot currently be written, but hasn't closed. Specifically
/// this means that the `write` function for types that implement this trait
/// can have a few return values:
///
/// * `Ok(n)` means that `n` bytes of data was immediately written .
/// * `Err(e) if e.kind() == ErrorKind::WouldBlock` means that no data was
///   written from the buffer provided. The I/O object is not currently
///   writable but may become writable in the future. Most importantly, **the
///   current future's task is scheduled to get unparked when the object is
///   readable**. This means that like `Future::poll` you'll receive a
///   notification when the I/O object is writable again.
/// * `Err(e)` for other errors are standard I/O errors coming from the
///   underlying object.
///
/// This trait importantly means that the `write` method only works in the
/// context of a future's task. The object may panic if used outside of a task.
///
/// Note that this trait also represents that the  `Write::flush` method works
/// very similarly to the `write` method, notably that `Ok(())` means that the
/// writer has successfully been flushed, a "would block" error means that the
/// current task is ready to receive a notification when flushing can make more
/// progress, and otherwise normal errors can happen as well.
pub trait AsyncWrite: std_io::Write {
    /// Initiates or attempts to shut down this writer, returning success when
    /// the I/O connection has completely shut down.
    ///
    /// This method is intended to be used for asynchronous shutdown of I/O
    /// connections. For example this is suitable for implementing shutdown of a
    /// TLS connection or calling `TcpStream::shutdown` on a proxied connection.
    /// Protocols sometimes need to flush out final pieces of data or otherwise
    /// perform a graceful shutdown handshake, reading/writing more data as
    /// appropriate. This method is the hook for such protocols to implement the
    /// graceful shutdown logic.
    ///
    /// This `shutdown` method is required by implementors of the
    /// `AsyncWrite` trait. Wrappers typically just want to proxy this call
    /// through to the wrapped type, and base types will typically implement
    /// shutdown logic here or just return `Ok(().into())`. Note that if you're
    /// wrapping an underlying `AsyncWrite` a call to `shutdown` implies that
    /// transitively the entire stream has been shut down. After your wrapper's
    /// shutdown logic has been executed you should shut down the underlying
    /// stream.
    ///
    /// Invocation of a `shutdown` implies an invocation of `flush`. Once this
    /// method returns `Ready` it implies that a flush successfully happened
    /// before the shutdown happened. That is, callers don't need to call
    /// `flush` before calling `shutdown`. They can rely that by calling
    /// `shutdown` any pending buffered data will be written out.
    ///
    /// # Return value
    ///
    /// This function returns a `Poll<(), io::Error>` classified as such:
    ///
    /// * `Ok(Async::Ready(()))` - indicates that the connection was
    ///   successfully shut down and is now safe to deallocate/drop/close
    ///   resources associated with it. This method means that the current task
    ///   will no longer receive any notifications due to this method and the
    ///   I/O object itself is likely no longer usable.
    ///
    /// * `Ok(Async::NotReady)` - indicates that shutdown is initiated but could
    ///   not complete just yet. This may mean that more I/O needs to happen to
    ///   continue this shutdown operation. The current task is scheduled to
    ///   receive a notification when it's otherwise ready to continue the
    ///   shutdown operation. When woken up this method should be called again.
    ///
    /// * `Err(e)` - indicates a fatal error has happened with shutdown,
    ///   indicating that the shutdown operation did not complete successfully.
    ///   This typically means that the I/O object is no longer usable.
    ///
    /// # Errors
    ///
    /// This function can return normal I/O errors through `Err`, described
    /// above. Additionally this method may also render the underlying
    /// `Write::write` method no longer usable (e.g. will return errors in the
    /// future). It's recommended that once `shutdown` is called the
    /// `write` method is no longer called.
    ///
    /// # Panics
    ///
    /// This function will panic if not called within the context of a future's
    /// task.
    fn shutdown(&mut self) -> Poll<(), std_io::Error>;

    /// Write a `Buf` into this value, returning how many bytes were written.
    ///
    /// Note that this method will advance the `buf` provided automatically by
    /// the number of bytes written.
    fn write_buf<B: Buf>(&mut self, buf: &mut B) -> Poll<usize, std_io::Error>
        where Self: Sized,
    {
        if !buf.has_remaining() {
            return Ok(Async::Ready(0));
        }

        let n = try_nb!(self.write(buf.bytes()));
        buf.advance(n);
        Ok(Async::Ready(n))
    }
}

impl<T: ?Sized + AsyncWrite> AsyncWrite for Box<T> {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        (**self).shutdown()
    }
}
impl<'a, T: ?Sized + AsyncWrite> AsyncWrite for &'a mut T {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        (**self).shutdown()
    }
}

impl AsyncRead for std_io::Repeat {
    unsafe fn prepare_uninitialized_buffer(&self, _: &mut [u8]) -> bool {
        false
    }
}

impl AsyncWrite for std_io::Sink {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        Ok(().into())
    }
}

// TODO: Implement `prepare_uninitialized_buffer` for `io::Take`.
// This is blocked on rust-lang/rust#27269
impl<T: AsyncRead> AsyncRead for std_io::Take<T> {
}

// TODO: Implement `prepare_uninitialized_buffer` when upstream exposes inner
// parts
impl<T, U> AsyncRead for std_io::Chain<T, U>
    where T: AsyncRead,
          U: AsyncRead,
{
}

impl<T: AsyncWrite> AsyncWrite for std_io::BufWriter<T> {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        try_nb!(self.flush());
        self.get_mut().shutdown()
    }
}

impl<T: AsyncRead> AsyncRead for std_io::BufReader<T> {
    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [u8]) -> bool {
        self.get_ref().prepare_uninitialized_buffer(buf)
    }
}

impl<T: AsRef<[u8]>> AsyncRead for std_io::Cursor<T> {
}

impl<'a> AsyncWrite for std_io::Cursor<&'a mut [u8]> {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        Ok(().into())
    }
}

impl AsyncWrite for std_io::Cursor<Vec<u8>> {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        Ok(().into())
    }
}

impl AsyncWrite for std_io::Cursor<Box<[u8]>> {
    fn shutdown(&mut self) -> Poll<(), std_io::Error> {
        Ok(().into())
    }
}

fn _assert_objects() {
    fn _assert<T>() {}
    _assert::<Box<AsyncRead>>();
    _assert::<Box<AsyncWrite>>();
}
