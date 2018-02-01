use std::{fmt, io};

use AsyncRead;
use framed::Fuse;

use futures::{Async, Poll, Stream, Sink, StartSend};
use bytes::BytesMut;

/// Decoding of frames via buffers.
///
/// This trait is used when constructing an instance of `Framed` or
/// `FramedRead`. An implementation of `Decoder` takes a byte stream that has
/// already been buffered in `src` and decodes the data into a stream of
/// `Self::Item` frames.
///
/// Implementations are able to track state on `self`, which enables
/// implementing stateful streaming parsers. In many cases, though, this type
/// will simply be a unit struct (e.g. `struct HttpDecoder`).
pub trait Decoder {
    /// The type of decoded frames.
    type Item;

    /// The type of unrecoverable frame decoding errors.
    ///
    /// If an individual message is ill-formed but can be ignored without
    /// interfering with the processing of future messages, it may be more
    /// useful to report the failure as an `Item`.
    ///
    /// `From<io::Error>` is required in the interest of making `Error` suitable
    /// for returning directly from a `FramedRead`, and to enable the default
    /// implementation of `decode_eof` to yield an `io::Error` when the decoder
    /// fails to consume all available data.
    ///
    /// Note that implementors of this trait can simply indicate `type Error =
    /// io::Error` to use I/O errors as this type.
    type Error: From<io::Error>;

    /// Attempts to decode a frame from the provided buffer of bytes.
    ///
    /// This method is called by `FramedRead` whenever bytes are ready to be
    /// parsed.  The provided buffer of bytes is what's been read so far, and
    /// this instance of `Decode` can determine whether an entire frame is in
    /// the buffer and is ready to be returned.
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
    /// Note that the bytes provided may be empty. If a previous call to
    /// `decode` consumed all the bytes in the buffer then `decode` will be
    /// called again until it returns `None`, indicating that more bytes need to
    /// be read.
    ///
    /// Finally, if the bytes in the buffer are malformed then an error is
    /// returned indicating why. This informs `Framed` that the stream is now
    /// corrupt and should be terminated.
    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error>;

    /// A default method available to be called when there are no more bytes
    /// available to be read from the underlying I/O.
    ///
    /// This method defaults to calling `decode` and returns an error if
    /// `Ok(None)` is returned while there is unconsumed data in `buf`.
    /// Typically this doesn't need to be implemented unless the framing
    /// protocol differs near the end of the stream.
    ///
    /// Note that the `buf` argument may be empty. If a previous call to
    /// `decode_eof` consumed all the bytes in the buffer, `decode_eof` will be
    /// called again until it returns `None`, indicating that there are no more
    /// frames to yield. This behavior enables returning finalization frames
    /// that may not be based on inbound data.
    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        match try!(self.decode(buf)) {
            Some(frame) => Ok(Some(frame)),
            None => {
                if buf.is_empty() {
                    Ok(None)
                } else {
                    Err(io::Error::new(io::ErrorKind::Other,
                                       "bytes remaining on stream").into())
                }
            }
        }
    }
}

/// A `Stream` of messages decoded from an `AsyncRead`.
pub struct FramedRead<T, D> {
    inner: FramedRead2<Fuse<T, D>>,
}

pub struct FramedRead2<T> {
    inner: T,
    eof: bool,
    is_readable: bool,
    buffer: BytesMut,
}

const INITIAL_CAPACITY: usize = 8 * 1024;

// ===== impl FramedRead =====

impl<T, D> FramedRead<T, D>
    where T: AsyncRead,
          D: Decoder,
{
    /// Creates a new `FramedRead` with the given `decoder`.
    pub fn new(inner: T, decoder: D) -> FramedRead<T, D> {
        FramedRead {
            inner: framed_read2(Fuse(inner, decoder)),
        }
    }
}

impl<T, D> FramedRead<T, D> {
    /// Returns a reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_ref(&self) -> &T {
        &self.inner.inner.0
    }

    /// Returns a mutable reference to the underlying I/O stream wrapped by
    /// `FramedRead`.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner.inner.0
    }

    /// Consumes the `FramedRead`, returning its underlying I/O stream.
    ///
    /// Note that care should be taken to not tamper with the underlying stream
    /// of data coming in as it may corrupt the stream of frames otherwise
    /// being worked with.
    pub fn into_inner(self) -> T {
        self.inner.inner.0
    }

    /// Returns a reference to the underlying decoder.
    pub fn decoder(&self) -> &D {
        &self.inner.inner.1
    }

    /// Returns a mutable reference to the underlying decoder.
    pub fn decoder_mut(&mut self) -> &mut D {
        &mut self.inner.inner.1
    }
}

impl<T, D> Stream for FramedRead<T, D>
    where T: AsyncRead,
          D: Decoder,
{
    type Item = D::Item;
    type Error = D::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.inner.poll()
    }
}

impl<T, D> Sink for FramedRead<T, D>
    where T: Sink,
{
    type SinkItem = T::SinkItem;
    type SinkError = T::SinkError;

    fn start_send(&mut self,
                  item: Self::SinkItem)
                  -> StartSend<Self::SinkItem, Self::SinkError>
    {
        self.inner.inner.0.start_send(item)
    }

    fn poll_complete(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.inner.0.poll_complete()
    }

    fn close(&mut self) -> Poll<(), Self::SinkError> {
        self.inner.inner.0.close()
    }
}

impl<T, D> fmt::Debug for FramedRead<T, D>
    where T: fmt::Debug,
          D: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FramedRead")
            .field("inner", &self.inner.inner.0)
            .field("decoder", &self.inner.inner.1)
            .field("eof", &self.inner.eof)
            .field("is_readable", &self.inner.is_readable)
            .field("buffer", &self.inner.buffer)
            .finish()
    }
}

// ===== impl FramedRead2 =====

pub fn framed_read2<T>(inner: T) -> FramedRead2<T> {
    FramedRead2 {
        inner: inner,
        eof: false,
        is_readable: false,
        buffer: BytesMut::with_capacity(INITIAL_CAPACITY),
    }
}

pub fn framed_read2_with_buffer<T>(inner: T, mut buf: BytesMut) -> FramedRead2<T> {
    if buf.capacity() < INITIAL_CAPACITY {
        let bytes_to_reserve = INITIAL_CAPACITY - buf.capacity();
        buf.reserve(bytes_to_reserve);
    }
    FramedRead2 {
        inner: inner,
        eof: false,
        is_readable: buf.len() > 0,
        buffer: buf,
    }
}

impl<T> FramedRead2<T> {
    pub fn get_ref(&self) -> &T {
        &self.inner
    }

    pub fn into_inner(self) -> T {
        self.inner
    }

    pub fn into_parts(self) -> (T, BytesMut) {
        (self.inner, self.buffer)
    }

    pub fn get_mut(&mut self) -> &mut T {
        &mut self.inner
    }
}

impl<T> Stream for FramedRead2<T>
    where T: AsyncRead + Decoder,
{
    type Item = T::Item;
    type Error = T::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            // Repeatedly call `decode` or `decode_eof` as long as it is
            // "readable". Readable is defined as not having returned `None`. If
            // the upstream has returned EOF, and the decoder is no longer
            // readable, it can be assumed that the decoder will never become
            // readable again, at which point the stream is terminated.
            if self.is_readable {
                if self.eof {
                    let frame = try!(self.inner.decode_eof(&mut self.buffer));
                    return Ok(Async::Ready(frame));
                }

                trace!("attempting to decode a frame");

                if let Some(frame) = try!(self.inner.decode(&mut self.buffer)) {
                    trace!("frame decoded from buffer");
                    return Ok(Async::Ready(Some(frame)));
                }

                self.is_readable = false;
            }

            assert!(!self.eof);

            // Otherwise, try to read more data and try again. Make sure we've
            // got room for at least one byte to read to ensure that we don't
            // get a spurious 0 that looks like EOF
            self.buffer.reserve(1);
            if 0 == try_ready!(self.inner.read_buf(&mut self.buffer)) {
                self.eof = true;
            }

            self.is_readable = true;
        }
    }
}
