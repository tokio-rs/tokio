//! A module for working with "easy" types to interact with other parts of
//! tokio-core.
//!
//! This module contains a number of concrete implementations of various
//! abstractions in tokio-core. The contents of this module are not necessarily
//! production ready but are intended to allow projects to get off the ground
//! quickly while also showing off sample implementations of these traits.
//!
//! Currently this module primarily contains `EasyFramed`, a struct which
//! implements the `FramedIo` trait in `tokio_core::io`. This structure allows
//! simply defining a parser (via the `Parse` trait) and a serializer (via the
//! `Serialize` trait) and transforming a stream of bytes into a stream of
//! frames. Additionally the `Parse` trait passes an `EasyBuf`, another type
//! here, which primarily supports `drain_to`, to extract bytes without copying
//! them.
//!
//! For more information see the `EasyFramed` and `EasyBuf` types.

use std::io;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use futures::{Async, Poll};

use io::{Io, FramedIo};

/// A reference counted buffer of bytes.
///
/// An `EasyBuf` is a representation of a byte buffer where sub-slices of it can
/// be handed out efficiently, each with a `'static` lifetime which keeps the
/// data alive. The buffer also supports mutation but may require bytes to be
/// copied to complete the operation.
pub struct EasyBuf {
    buf: Arc<Vec<u8>>,
    start: usize,
    end: usize,
}

/// An RAII object returned from `get_mut` which provides mutable access to the
/// underlying `Vec<u8>`.
pub struct EasyBufMut<'a> {
    buf: &'a mut Vec<u8>,
    end: &'a mut usize,
}

impl EasyBuf {
    /// Creates a new EasyBuf with no data and the default capacity.
    pub fn new() -> EasyBuf {
        EasyBuf::with_capacity(8 * 1024)
    }

    /// Creates a new EasyBuf with `cap` capacity.
    pub fn with_capacity(cap: usize) -> EasyBuf {
        EasyBuf {
            buf: Arc::new(Vec::with_capacity(cap)),
            start: 0,
            end: 0,
        }
    }

    /// Changes the starting index of this window to the index specified.
    ///
    /// Returns the windows back to chain multiple calls to this method.
    ///
    /// # Panics
    ///
    /// This method will panic if `start` is out of bounds for the underlying
    /// slice or if it comes after the `end` configured in this window.
    fn set_start(&mut self, start: usize) -> &mut EasyBuf {
        assert!(start <= self.buf.as_ref().len());
        assert!(start <= self.end);
        self.start = start;
        self
    }

    /// Changes the end index of this window to the index specified.
    ///
    /// Returns the windows back to chain multiple calls to this method.
    ///
    /// # Panics
    ///
    /// This method will panic if `end` is out of bounds for the underlying
    /// slice or if it comes after the `end` configured in this window.
    fn set_end(&mut self, end: usize) -> &mut EasyBuf {
        assert!(end <= self.buf.len());
        assert!(self.start <= end);
        self.end = end;
        self
    }

    /// Returns the number of bytes contained in this `EasyBuf`.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// Returns the inner contents of this `EasyBuf` as a slice.
    pub fn as_slice(&self) -> &[u8] {
        self.as_ref()
    }

    /// Splits the buffer into two at the given index.
    ///
    /// Afterwards `self` contains elements `[0, at)`, and the returned `EasyBuf`
    /// contains elements `[at, len)`.
    ///
    /// This is an O(1) operation that just increases the reference count and
    /// sets a few indexes.
    ///
    /// # Panics
    ///
    /// Panics if `at > len`
    pub fn split_off(&mut self, at: usize) -> EasyBuf {
        let mut other = EasyBuf { buf: self.buf.clone(), ..*self };
        let idx = self.start + at;
        other.set_start(idx);
        self.set_end(idx);
        return other
    }

    /// Splits the buffer into two at the given index.
    ///
    /// Afterwards `self` contains elements `[at, len)`, and the returned `EasyBuf`
    /// contains elements `[0, at)`.
    ///
    /// This is an O(1) operation that just increases the reference count and
    /// sets a few indexes.
    ///
    /// # Panics
    ///
    /// Panics if `at > len`
    pub fn drain_to(&mut self, at: usize) -> EasyBuf {
        let mut other = EasyBuf { buf: self.buf.clone(), ..*self };
        let idx = self.start + at;
        other.set_end(idx);
        self.set_start(idx);
        return other
    }

    /// Returns a mutable reference to the underlying growable buffer of bytes.
    ///
    /// If this `EasyBuf` is the only instance pointing at the underlying buffer
    /// of bytes, a direct mutable reference will be returned. Otherwise the
    /// contents of this `EasyBuf` will be reallocated in a fresh `Vec<u8>`
    /// allocation with the same capacity as this allocation, and that
    /// allocation will be returned.
    ///
    /// This operation **is not O(1)** as it may clone the entire contents of
    /// this buffer.
    ///
    /// The returned `EasyBufMut` type implement `Deref` and `DerefMut` to
    /// `Vec<u8>` can the byte buffer can be manipulated using the standard
    /// `Vec<u8>` methods.
    pub fn get_mut(&mut self) -> EasyBufMut {
        // Fast path if we can get mutable access to our own current
        // buffer.
        //
        // TODO: this should be a match or an if-let
        if Arc::get_mut(&mut self.buf).is_some() {
            let buf = Arc::get_mut(&mut self.buf).unwrap();
            buf.drain(..self.start);
            self.start = 0;
            return EasyBufMut { buf: buf, end: &mut self.end }
        }

        // If we couldn't get access above then we give ourself a new buffer
        // here.
        let mut v = Vec::with_capacity(self.buf.capacity());
        v.extend_from_slice(self.as_ref());
        self.start = 0;
        self.buf = Arc::new(v);
        EasyBufMut {
            buf: Arc::get_mut(&mut self.buf).unwrap(),
            end: &mut self.end,
        }
    }
}

impl AsRef<[u8]> for EasyBuf {
    fn as_ref(&self) -> &[u8] {
        &self.buf[self.start..self.end]
    }
}

impl<'a> Deref for EasyBufMut<'a> {
    type Target = Vec<u8>;

    fn deref(&self) -> &Vec<u8> {
        self.buf
    }
}

impl<'a> DerefMut for EasyBufMut<'a> {
    fn deref_mut(&mut self) -> &mut Vec<u8> {
        self.buf
    }
}

impl<'a> Drop for EasyBufMut<'a> {
    fn drop(&mut self) {
        *self.end = self.buf.len();
    }
}

/// An implementation of the `FramedIo` trait building on instances of the
/// `Parse` and `Serialize` traits.
///
/// Many I/O streams are simply a framed protocol on both the inbound and
/// outbound halves. In essence the underlying stream of bytes can be converted
/// to a stream of *frames*. This way instead of reading or writing bytes a
/// stream deals with reading and writing frames.
///
/// This struct is essentially a convenience implementation of the `FramedIo`
/// which only requires knowledge of how to parse and serialize types. It is
/// constructed with an arbitrary `Io` instance along with how to parse and
/// serialize the frames that this `EasyFramed` will be yielding.
///
/// This implementation of `FramedIo` uses the `EasyBuf` type from the `bytes`
/// crate for the backing storage, which should allow for zero-copy parsing
/// where possible.
pub struct EasyFramed<T, P, S> {
    upstream: T,
    parse: P,
    serialize: S,
    eof: bool,
    is_readable: bool,
    rd: EasyBuf,
    wr: Vec<u8>,
}

/// Implementation of parsing a frame from an internal buffer.
///
/// This trait is used when constructing an instance of `EasyFramed`. It defines how
/// to parse the incoming bytes on a stream to the specified type of frame for
/// that framed I/O stream.
///
/// The primary method of this trait, `parse`, attempts to parse a method from a
/// buffer of bytes. It has the option of returning `NotReady`, indicating that
/// more bytes need to be read before parsing can continue as well.
pub trait Parse {

    /// The type that this instance of `Parse` will attempt to be parsing.
    ///
    /// This is typically a frame being parsed from an input stream, such as an
    /// HTTP request, a Redis command, etc.
    type Out;

    /// Attempts to parse a frame from the provided buffer of bytes.
    ///
    /// This method is called by `EasyFramed` whenever bytes are ready to be parsed.
    /// The provided buffer of bytes is what's been read so far, and this
    /// instance of `Parse` can determine whether an entire frame is in the
    /// buffer and is ready to be returned.
    ///
    /// If an entire frame is available, then this instance will remove those
    /// bytes from the buffer provided and return them as a parsed frame. Note
    /// that removing bytes from the provided buffer doesn't always necessarily
    /// copy the bytes, so this should be an efficient operation in most
    /// circumstances.
    ///
    /// If the bytes look valid, but a frame isn't fully available yet, then
    /// `Async::NotReady` is returned. This indicates to the `EasyFramed` instance
    /// that it needs to read some more bytes before calling this method again.
    ///
    /// Finally, if the bytes in the buffer are malformed then an error is
    /// returned indicating why. This informs `EasyFramed` that the stream is now
    /// corrupt and should be terminated.
    fn parse(&mut self, buf: &mut EasyBuf) -> Poll<Self::Out, io::Error>;

    /// A default method available to be called when there are no more bytes
    /// available to be read from the underlying I/O.
    ///
    /// This method defaults to calling `parse` and returns an error if
    /// `NotReady` is returned. Typically this doesn't need to be implemented
    /// unless the framing protocol differs near the end of the stream.
    fn done(&mut self, buf: &mut EasyBuf) -> io::Result<Self::Out> {
        match try!(self.parse(buf)) {
            Async::Ready(frame) => Ok(frame),
            Async::NotReady => Err(io::Error::new(io::ErrorKind::Other,
                                                  "bytes remaining on stream")),
        }
    }
}

/// A trait for serializing frames into a byte buffer.
///
/// This trait is used as a building block of `EasyFramed` to define how frames are
/// serialized into bytes to get passed to the underlying byte stream. Each
/// frame written to `EasyFramed` will be serialized with this trait to an internal
/// buffer. That buffer is then written out when possible to the underlying I/O
/// stream.
pub trait Serialize {

    /// The frame that's being serialized to a byte buffer.
    ///
    /// This type is the type of frame that's also being written to a `EasyFramed`.
    type In;

    /// Serializes a frame into the buffer provided.
    ///
    /// This method will serialize `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `EasyFramed` instance and
    /// will be written out when possible.
    fn serialize(&mut self, msg: Self::In, buf: &mut Vec<u8>);
}

impl<T, P, S> EasyFramed<T, P, S>
    where T: Io,
          P: Parse,
          S: Serialize,
{
    /// Creates a new instance of `EasyFramed` from the given component pieces.
    ///
    /// This method will create a new instance of `EasyFramed` which implements
    /// `FramedIo` for reading and writing frames from an underlying I/O stream.
    /// The `upstream` argument here is the byte-based I/O stream that it will
    /// be operating on. Data will be read from this stream and parsed with
    /// `parse` into frames. Frames written to this instance will be serialized
    /// by `serialize` and then written to `upstream`.
    ///
    /// The `rd` and `wr` buffers provided are used for reading and writing
    /// bytes and provide a small amount of control over how buffering happens.
    pub fn new(upstream: T,
               parse: P,
               serialize: S) -> EasyFramed<T, P, S> {

        trace!("creating new framed transport");
        EasyFramed {
            upstream: upstream,
            parse: parse,
            serialize: serialize,
            is_readable: false,
            eof: false,
            rd: EasyBuf::new(),
            wr: Vec::with_capacity(8 * 1024),
        }
    }

    /// Returns a reference to the underlying I/O stream wrapped by `EasyFramed`.
    pub fn get_ref(&self) -> &T {
        &self.upstream
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by `EasyFramed`.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.upstream
    }

    /// Consumes the `EasyFramed`, returning its underlying I/O stream.
    pub fn into_inner(self) -> T {
        self.upstream
    }
}

impl<T, P, S> FramedIo for EasyFramed<T, P, S>
    where T: Io,
          P: Parse,
          S: Serialize,
{
    type In = S::In;
    type Out = Option<P::Out>;

    fn poll_read(&mut self) -> Async<()> {
        if self.is_readable || self.upstream.poll_read().is_ready() {
            Async::Ready(())
        } else {
            Async::NotReady
        }
    }

    fn read(&mut self) -> Poll<Self::Out, io::Error> {
        loop {
            // If the read buffer has any pending data, then it could be
            // possible that `parse` will return a new frame. We leave it to
            // the parser to optimize detecting that more data is required.
            if self.is_readable {
                if self.eof {
                    if self.rd.len() == 0 {
                        return Ok(None.into())
                    } else {
                        let frame = try!(self.parse.done(&mut self.rd));
                        return Ok(Some(frame).into())
                    }
                }
                trace!("attempting to parse a frame");
                if let Async::Ready(frame) = try!(self.parse.parse(&mut self.rd)) {
                    trace!("frame parsed from buffer");
                    return Ok(Some(frame).into());
                }
                self.is_readable = false;
            }

            assert!(!self.eof);

            // Otherwise, try to read more data and try again
            //
            // TODO: shouldn't read_to_end, that may read a lot
            let before = self.rd.len();
            let ret = self.upstream.read_to_end(&mut self.rd.get_mut());
            match ret {
                Ok(_n) => self.eof = true,
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if self.rd.len() == before {
                        return Ok(Async::NotReady)
                    }
                }
                Err(e) => return Err(e),
            }
            self.is_readable = true;
        }
    }

    fn poll_write(&mut self) -> Async<()> {
        // Always accept writes and let the write buffer grow
        //
        // TODO: This may not be the best option for robustness, but for now it
        // makes the microbenchmarks happy.
        Async::Ready(())
    }

    fn write(&mut self, msg: Self::In) -> Poll<(), io::Error> {
        if !self.poll_write().is_ready() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput,
                                      "transport not currently writable"));
        }

        // Serialize the msg
        self.serialize.serialize(msg, &mut self.wr);

        // TODO: should provide some backpressure, such as when the buffer is
        //       too full this returns `NotReady` or something like that.
        Ok(Async::Ready(()))
    }

    fn flush(&mut self) -> Poll<(), io::Error> {
        // Try flushing the underlying IO
        try_nb!(self.upstream.flush());

        trace!("flushing framed transport");

        loop {
            if self.wr.len() == 0 {
                trace!("framed transport flushed");
                return Ok(Async::Ready(()));
            }

            trace!("writing; remaining={:?}", self.wr.len());

            let n = try_nb!(self.upstream.write(&self.wr));
            self.wr.drain(..n);
        }
    }
}

