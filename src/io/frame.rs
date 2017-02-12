use std::fmt;
use std::io;
use std::mem;
use std::cmp;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use futures::{Async, Poll, Stream, Sink, StartSend, AsyncSink};

use io::Io;

const INITIAL_CAPACITY: usize = 8 * 1024;

/// A reference counted buffer of bytes.
///
/// An `EasyBuf` is a representation of a byte buffer where sub-slices of it can
/// be handed out efficiently, each with a `'static` lifetime which keeps the
/// data alive. The buffer also supports mutation but may require bytes to be
/// copied to complete the operation.
#[derive(Clone)]
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
        EasyBuf::with_capacity(INITIAL_CAPACITY)
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
    /// allocation with the same capacity as an `EasyBuf` created with `EasyBuf::new()`,
    /// and that allocation will be returned.
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
            buf.drain(self.end..);
            buf.drain(..self.start);
            self.start = 0;
            return EasyBufMut { buf: buf, end: &mut self.end }
        }

        // If we couldn't get access above then we give ourself a new buffer
        // here.
        let mut v = Vec::with_capacity(cmp::max(INITIAL_CAPACITY, self.as_ref().len()));
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

impl From<Vec<u8>> for EasyBuf {
    fn from(vec: Vec<u8>) -> EasyBuf {
        let end = vec.len();
        EasyBuf {
            buf: Arc::new(vec),
            start: 0,
            end: end,
        }
    }
}

impl<'a> Drop for EasyBufMut<'a> {
    fn drop(&mut self) {
        *self.end = self.buf.len();
    }
}

impl fmt::Debug for EasyBuf {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        let bytes = self.as_ref();
        let len = self.len();
        if len < 10 {
            write!(formatter, "EasyBuf{{len={}/{} {:?}}}", self.len(), self.buf.len(), bytes)
        } else { // choose a more compact representation
            write!(formatter, "EasyBuf{{len={}/{} [{}, {}, {}, {}, ..., {}, {}, {}, {}]}}", self.len(), self.buf.len(), bytes[0], bytes[1], bytes[2], bytes[3], bytes[len-4], bytes[len-3], bytes[len-2], bytes[len-1])
        }
    }
}

impl Into<Vec<u8>> for EasyBuf {
    fn into(mut self) -> Vec<u8> {
        mem::replace(self.get_mut().buf, vec![])
    }
}

/// Encoding and decoding of frames via buffers.
///
/// This trait is used when constructing an instance of `Framed`. It provides
/// two types: `In`, for decoded input frames, and `Out`, for outgoing frames
/// that need to be encoded. It also provides methods to actually perform the
/// encoding and decoding, which work with corresponding buffer types.
///
/// The trait itself is implemented on a type that can track state for decoding
/// or encoding, which is particularly useful for streaming parsers. In many
/// cases, though, this type will simply be a unit struct (e.g. `struct
/// HttpCodec`).
pub trait Codec {
    /// The type of decoded frames.
    type In;

    /// The type of frames to be encoded.
    type Out;

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
    fn decode(&mut self, buf: &mut EasyBuf) -> io::Result<Option<Self::In>>;

    /// A default method available to be called when there are no more bytes
    /// available to be read from the underlying I/O.
    ///
    /// This method defaults to calling `decode` and returns an error if
    /// `Ok(None)` is returned. Typically this doesn't need to be implemented
    /// unless the framing protocol differs near the end of the stream.
    fn decode_eof(&mut self, buf: &mut EasyBuf) -> io::Result<Self::In> {
        match try!(self.decode(buf)) {
            Some(frame) => Ok(frame),
            None => Err(io::Error::new(io::ErrorKind::Other,
                                       "bytes remaining on stream")),
        }
    }

    /// Encodes a frame into the buffer provided.
    ///
    /// This method will encode `msg` into the byte buffer provided by `buf`.
    /// The `buf` provided is an internal buffer of the `Framed` instance and
    /// will be written out when possible.
    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> io::Result<()>;
}

/// A unified `Stream` and `Sink` interface to an underlying `Io` object, using
/// the `Codec` trait to encode and decode frames.
///
/// You can acquire a `Framed` instance by using the `Io::framed` adapter.
pub struct Framed<T, C> {
    upstream: T,
    codec: C,
    eof: bool,
    is_readable: bool,
    rd: EasyBuf,
    wr: Vec<u8>,
}

impl<T: Io, C: Codec> Stream for Framed<T, C> {
    type Item = C::In;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<C::In>, io::Error> {
        loop {
            // If the read buffer has any pending data, then it could be
            // possible that `decode` will return a new frame. We leave it to
            // the decoder to optimize detecting that more data is required.
            if self.is_readable {
                if self.eof {
                    if self.rd.len() == 0 {
                        return Ok(None.into())
                    } else {
                        let frame = try!(self.codec.decode_eof(&mut self.rd));
                        return Ok(Async::Ready(Some(frame)))
                    }
                }
                trace!("attempting to decode a frame");
                if let Some(frame) = try!(self.codec.decode(&mut self.rd)) {
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
}

impl<T: Io, C: Codec> Sink for Framed<T, C> {
    type SinkItem = C::Out;
    type SinkError = io::Error;

    fn start_send(&mut self, item: C::Out) -> StartSend<C::Out, io::Error> {
        // If the buffer is already over 8KiB, then attempt to flush it. If after flushing it's
        // *still* over 8KiB, then apply backpressure (reject the send).
        const BACKPRESSURE_BOUNDARY: usize = INITIAL_CAPACITY;
        if self.wr.len() > BACKPRESSURE_BOUNDARY {
            try!(self.poll_complete());
            if self.wr.len() > BACKPRESSURE_BOUNDARY {
                return Ok(AsyncSink::NotReady(item));
            }
        }

        try!(self.codec.encode(item, &mut self.wr));
        Ok(AsyncSink::Ready)
    }

    fn poll_complete(&mut self) -> Poll<(), io::Error> {
        trace!("flushing framed transport");

        while !self.wr.is_empty() {
            trace!("writing; remaining={}", self.wr.len());
            let n = try_nb!(self.upstream.write(&self.wr));
            if n == 0 {
                return Err(io::Error::new(io::ErrorKind::WriteZero,
                                          "failed to write frame to transport"));
            }
            self.wr.drain(..n);
        }

        // Try flushing the underlying IO
        try_nb!(self.upstream.flush());

        trace!("framed transport flushed");
        return Ok(Async::Ready(()));
    }
}

pub fn framed<T, C>(io: T, codec: C) -> Framed<T, C> {
    Framed {
        upstream: io,
        codec: codec,
        eof: false,
        is_readable: false,
        rd: EasyBuf::new(),
        wr: Vec::with_capacity(INITIAL_CAPACITY),
    }
}

impl<T, C> Framed<T, C> {

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

#[cfg(test)]
mod tests {
    use super::{INITIAL_CAPACITY, EasyBuf};
    use std::mem;

    #[test]
    fn debug_empty_easybuf() {
        let buf: EasyBuf = vec![].into();
        assert_eq!("EasyBuf{len=0/0 []}", format!("{:?}", buf));
    }

    #[test]
    fn debug_small_easybuf() {
        let buf: EasyBuf = vec![1, 2, 3, 4, 5, 6].into();
        assert_eq!("EasyBuf{len=6/6 [1, 2, 3, 4, 5, 6]}", format!("{:?}", buf));
    }

    #[test]
    fn debug_small_easybuf_split() {
        let mut buf: EasyBuf = vec![1, 2, 3, 4, 5, 6].into();
        let split = buf.split_off(4);
        assert_eq!("EasyBuf{len=4/6 [1, 2, 3, 4]}", format!("{:?}", buf));
        assert_eq!("EasyBuf{len=2/6 [5, 6]}", format!("{:?}", split));
    }

    #[test]
    fn debug_large_easybuf() {
        let vec: Vec<u8> = (0u8..255u8).collect();
        let buf: EasyBuf = vec.into();
        assert_eq!("EasyBuf{len=255/255 [0, 1, 2, 3, ..., 251, 252, 253, 254]}", format!("{:?}", buf));
    }

    #[test]
    fn easybuf_get_mut_sliced() {
        let vec: Vec<u8> = (0u8..10u8).collect();
        let mut buf: EasyBuf = vec.into();
        buf.split_off(9);
        buf.drain_to(3);
        assert_eq!(*buf.get_mut(), [3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn easybuf_get_mut_sliced_allocating_at_least_initial_capacity() {
        let vec: Vec<u8> = (0u8..10u8).collect();
        let mut buf: EasyBuf = vec.into();
        buf.split_off(9);
        buf.drain_to(3);
        // Clone to make shared
        let clone = buf.clone();
        assert_eq!(*buf.get_mut(), [3, 4, 5, 6, 7, 8]);
        assert_eq!(buf.get_mut().buf.capacity(), INITIAL_CAPACITY);
        mem::drop(clone); // prevent unused warning
    }

    #[test]
    fn easybuf_get_mut_sliced_allocating_required_capacity() {
        let vec: Vec<u8> = (0..INITIAL_CAPACITY * 2).map(|_|0u8).collect();
        let mut buf: EasyBuf = vec.into();
        buf.drain_to(INITIAL_CAPACITY / 2);
        let clone = buf.clone();
        assert_eq!(buf.get_mut().buf.capacity(), INITIAL_CAPACITY + INITIAL_CAPACITY / 2);
        mem::drop(clone)
    }

    #[test]
    fn easybuf_into_vec_simple() {
        let vec: Vec<u8> = (0u8..10u8).collect();
        let reference = vec.clone();
        let buf: EasyBuf = vec.into();
        let original_pointer = buf.buf.as_ref().as_ptr();
        let result: Vec<u8> = buf.into();
        assert_eq!(result, reference);
        let new_pointer = result.as_ptr();
        assert_eq!(original_pointer, new_pointer, "Into<Vec<u8>> should reuse the exclusive Vec");
    }

    #[test]
    fn easybuf_into_vec_sliced() {
        let vec: Vec<u8> = (0u8..10u8).collect();
        let mut buf: EasyBuf = vec.into();
        let original_pointer = buf.buf.as_ref().as_ptr();
        buf.split_off(9);
        buf.drain_to(3);
        let result: Vec<u8> = buf.into();
        let reference: Vec<u8> = (3u8..9u8).collect();
        assert_eq!(result, reference);
        let new_pointer = result.as_ptr();
        assert_eq!(original_pointer, new_pointer, "Into<Vec<u8>> should reuse the exclusive Vec");
    }

    #[test]
    fn easybuf_into_vec_sliced_allocating() {
        let vec: Vec<u8> = (0u8..10u8).collect();
        let mut buf: EasyBuf = vec.into();
        let original_pointer = buf.buf.as_ref().as_ptr();
        // Create a clone to create second reference to this EasyBuf and force allocation
        let original = buf.clone();
        buf.split_off(9);
        buf.drain_to(3);
        let result: Vec<u8> = buf.into();
        let reference: Vec<u8> = (3u8..9u8).collect();
        assert_eq!(result, reference);
        let original_reference: EasyBuf =(0u8..10u8).collect::<Vec<u8>>().into();
        assert_eq!(original.as_ref(), original_reference.as_ref());
        let new_pointer = result.as_ptr();
        assert_ne!(original_pointer, new_pointer, "A new vec should be allocated");
    }

}
