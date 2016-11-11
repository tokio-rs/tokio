use std::io;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use futures::{Async, Poll, Stream, Sink, StartSend, AsyncSink};
use futures::sync::BiLock;

use io::Io;

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

/// Decoding of a frame from an internal buffer.
///
/// This trait is used when constructing an instance of `Framed`. It defines how
/// to decode the incoming bytes on a stream to the specified type of frame for
/// that framed I/O stream.
///
/// The primary method of this trait, `decode`, attempts to decode a
/// frame from a buffer of bytes. It has the option of returning `NotReady`,
/// indicating that more bytes need to be read before decoding can
/// continue.
pub trait Decode: Sized {
    /// Attempts to decode a frame from the provided buffer of bytes.
    ///
    /// This method is called by `Framed` whenever bytes are ready to be parsed.
    /// The provided buffer of bytes is what's been read so far, and this
    /// instance of `Decode` can determine whether an entire frame is in the
    /// buffer and is ready to be returned.
    ///
    /// If an entire frame is available, then this instance will remove those
    /// bytes from the buffer provided and return them as a decoded
    /// frame. Note that removing bytes from the provided buffer doesn't always
    /// necessarily copy the bytes, so this should be an efficient operation in
    /// most circumstances.
    ///
    /// If the bytes look valid, but a frame isn't fully available yet, then
    /// `Ok(None)` is returned. This indicates to the `Framed` instance that
    /// it needs to read some more bytes before calling this method again.
    ///
    /// Finally, if the bytes in the buffer are malformed then an error is
    /// returned indicating why. This informs `Framed` that the stream is now
    /// corrupt and should be terminated.
    fn decode(buf: &mut EasyBuf) -> Result<Option<Self>, io::Error>;

    /// A default method available to be called when there are no more bytes
    /// available to be read from the underlying I/O.
    ///
    /// This method defaults to calling `decode` and returns an error if
    /// `Ok(None)` is returned. Typically this doesn't need to be implemented
    /// unless the framing protocol differs near the end of the stream.
    fn done(buf: &mut EasyBuf) -> io::Result<Self> {
        match try!(Self::decode(buf)) {
            Some(frame) => Ok(frame),
            None => Err(io::Error::new(io::ErrorKind::Other,
                                       "bytes remaining on stream")),
        }
    }
}

/// A trait for encoding frames into a byte buffer.
///
/// This trait is used as a building block of `Framed` to define how frames are
/// encoded into bytes to get passed to the underlying byte stream. Each
/// frame written to `Framed` will be encoded with this trait to an internal
/// buffer. That buffer is then written out when possible to the underlying I/O
/// stream.
pub trait Encode {
    /// Encodes a frame into the buffer provided.
    ///
    /// This method will encode `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `Framed` instance and
    /// will be written out when possible.
    fn encode(self, buf: &mut Vec<u8>);
}

struct ReadState {
    eof: bool,
    is_readable: bool,
    rd: EasyBuf,
}

impl ReadState {
    fn new() -> ReadState {
        ReadState {
            eof: false,
            is_readable: false,
            rd: EasyBuf::new(),
        }
    }
}

impl ReadState {
    fn poll<T: Io, D: Decode>(&mut self, upstream: &mut T) -> Poll<Option<D>, io::Error> {
        loop {
            // If the read buffer has any pending data, then it could be
            // possible that `decode` will return a new frame. We leave it to
            // the decoder to optimize detecting that more data is required.
            if self.is_readable {
                if self.eof {
                    if self.rd.len() == 0 {
                        return Ok(None.into())
                    } else {
                        let frame = try!(Decode::done(&mut self.rd));
                        return Ok(Async::Ready(Some(frame)))
                    }
                }
                trace!("attempting to decode a frame");
                if let Some(frame) = try!(Decode::decode(&mut self.rd)) {
                    trace!("frame decoded from buffer");
                    return Ok(Async::Ready(Some(frame)));
                }
                self.is_readable = false;
            }

            assert!(!self.eof);

            // Otherwise, try to read more data and try again
            //
            // TODO: shouldn't read_to_end, that may read a lot
            let before = self.rd.len();
            let ret = upstream.read_to_end(&mut self.rd.get_mut());
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
}

struct WriteState {
    wr: Vec<u8>,
}

impl WriteState {
    fn new() -> WriteState {
        WriteState {
            wr: Vec::with_capacity(8 * 1024),
        }
    }
}

impl WriteState {
    fn write<E: Encode>(&mut self, data: E) {
        data.encode(&mut self.wr)
    }

    fn poll_complete<T: Io>(&mut self, upstream: &mut T) -> Poll<(), io::Error> {
        // Try flushing the underlying IO
        try_nb!(upstream.flush());

        trace!("flushing framed transport");

        loop {
            if self.wr.len() == 0 {
                trace!("framed transport flushed");
                return Ok(Async::Ready(()));
            }

            trace!("writing; remaining={:?}", self.wr.len());

            let n = try_nb!(upstream.write(&self.wr));
            self.wr.drain(..n);
        }
    }
}

/// A `Stream` interface to an underlying `Io` object, using the `Decode` trait
/// to decode frames.
pub struct FramedRead<T, D> {
    upstream: BiLock<T>,
    read_state: ReadState,
    _phantom: PhantomData<D>,
}

impl<T: Io, D: Decode> Stream for FramedRead<T, D> {
    type Item = D;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<D>, io::Error> {
        if let Async::Ready(mut guard) = self.upstream.poll_lock() {
            self.read_state.poll(&mut *guard)
        } else {
            Ok(Async::NotReady)
        }
    }
}

/// A `Sink` interface to an underlying `Io` object, using the `Encode` trait
/// to encode frames.
pub struct FramedWrite<T, E> {
    upstream: BiLock<T>,
    write_state: WriteState,
    _phantom: PhantomData<E>,
}

impl<T: Io, E: Encode> Sink for FramedWrite<T, E> {
    type SinkItem = E;
    type SinkError = io::Error;

    fn start_send(&mut self, item: E) -> StartSend<E, io::Error> {
        self.write_state.write(item);
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), io::Error> {
        if let Async::Ready(mut guard) = self.upstream.poll_lock() {
            self.write_state.poll_complete(&mut *guard)
        } else {
            Ok(Async::NotReady)
        }
    }
}

/// A unified `Stream` and `Sink` interface to an underlying `Io` object, using
/// the `Encode` and `Decode` traits to encode and decode frames.
///
/// You can acquire a `Framed` instance by using the `Io::framed` adapter.
pub struct Framed<T, D, E> {
    upstream: T,
    read_state: ReadState,
    write_state: WriteState,
    _phantom: PhantomData<(D, E)>,
}

impl<T: Io, D: Decode, E: Encode> Stream for Framed<T, D, E> {
    type Item = D;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<D>, io::Error> {
        self.read_state.poll(&mut self.upstream)
    }
}

impl<T: Io, D: Decode, E: Encode> Sink for Framed<T, D, E> {
    type SinkItem = E;
    type SinkError = io::Error;

    fn start_send(&mut self, item: E) -> StartSend<E, io::Error> {
        self.write_state.write(item);
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), io::Error> {
        self.write_state.poll_complete(&mut self.upstream)
    }
}

pub fn framed<T, D, E>(io: T) -> Framed<T, D, E> {
    Framed {
        upstream: io,
        read_state: ReadState::new(),
        write_state: WriteState::new(),
        _phantom: PhantomData,
    }
}

impl<T, D, E> Framed<T, D, E> {
    /// Splits this `Stream + Sink` object into separate `Stream` and `Sink`
    /// objects, which can be useful when you want to split ownership between
    /// tasks, or allow direct interaction between the two objects (e.g. via
    /// `Sink::send_all`).
    pub fn split(self) -> (FramedRead<T, D>, FramedWrite<T, E>) {
        let (a, b) = BiLock::new(self.upstream);
        let read = FramedRead {
            upstream: a,
            read_state: ReadState::new(),
            _phantom: PhantomData,
        };
        let write = FramedWrite {
            upstream: b,
            write_state: WriteState::new(),
            _phantom: PhantomData,
        };
        (read, write)
    }

    /// Returns a reference to the underlying I/O stream wrapped by `Framed`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise being
    /// worked with.
    pub fn get_ref(&self) -> &T {
        &self.upstream
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `Framed`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise being
    /// worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.upstream
    }

    /// Consumes the `Framed`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise being
    /// worked with.
    pub fn into_inner(self) -> T {
        self.upstream
    }
}
