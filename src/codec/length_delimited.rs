//! Frame a stream of bytes based on a length prefix
//!
//! Many protocols delimit their frames by prefacing frame data with a
//! frame head that specifies the length of the frame. The
//! `length_delimited` module provides utilities for handling the length
//! based framing. This allows the consumer to work with entire frames
//! without having to worry about buffering or other framing logic.
//!
//! # Getting started
//!
//! If implementing a protocol from scratch, using length delimited framing
//! is an easy way to get started. [`LengthDelimitedCodec::new()`] will
//! return a length delimited codec using default configuration values.
//! This can then be used to construct a framer to adapt a full-duplex
//! byte stream into a stream of frames.
//!
//! ```
//! # extern crate tokio;
//! use tokio::io::{AsyncRead, AsyncWrite};
//! use tokio::codec::*;
//!
//! fn bind_transport<T: AsyncRead + AsyncWrite>(io: T)
//!     -> Framed<T, LengthDelimitedCodec>
//! {
//!     Framed::new(io, LengthDelimitedCodec::new())
//! }
//! # pub fn main() {}
//! ```
//!
//! The returned transport implements `Sink + Stream` for `BytesMut`. It
//! encodes the frame with a big-endian `u32` header denoting the frame
//! payload length:
//!
//! ```text
//! +----------+--------------------------------+
//! | len: u32 |          frame payload         |
//! +----------+--------------------------------+
//! ```
//!
//! Specifically, given the following:
//!
//! ```
//! # extern crate tokio;
//! # extern crate bytes;
//! # extern crate futures;
//! #
//! use tokio::io::{AsyncRead, AsyncWrite};
//! use tokio::codec::*;
//! use bytes::Bytes;
//! use futures::{Sink, Future};
//!
//! fn write_frame<T: AsyncRead + AsyncWrite>(io: T) -> Result<(), Box<std::error::Error>> {
//!     let mut transport = Framed::new(io, LengthDelimitedCodec::new());
//!     let frame = Bytes::from("hello world");
//!
//!     transport.send(frame).wait()?;
//!     Ok(())
//! }
//! #
//! # pub fn main() {}
//! ```
//!
//! The encoded frame will look like this:
//!
//! ```text
//! +---- len: u32 ----+---- data ----+
//! | \x00\x00\x00\x0b |  hello world |
//! +------------------+--------------+
//! ```
//!
//! # Decoding
//!
//! [`FramedRead`] adapts an [`AsyncRead`] into a `Stream` of [`BytesMut`],
//! such that each yielded [`BytesMut`] value contains the contents of an
//! entire frame. There are many configuration parameters enabling
//! [`FramedRead`] to handle a wide range of protocols. Here are some
//! examples that will cover the various options at a high level.
//!
//! ## Example 1
//!
//! The following will parse a `u16` length field at offset 0, including the
//! frame head in the yielded `BytesMut`.
//!
//! ```
//! # extern crate tokio;
//! # use tokio::io::AsyncRead;
//! # use tokio::codec::length_delimited;
//! # fn bind_read<T: AsyncRead>(io: T) {
//! length_delimited::Builder::new()
//!     .length_field_offset(0) // default value
//!     .length_field_length(2)
//!     .length_adjustment(0)   // default value
//!     .num_skip(0) // Do not strip frame header
//!     .new_read(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! The following frame will be decoded as such:
//!
//! ```text
//!          INPUT                           DECODED
//! +-- len ---+--- Payload ---+     +-- len ---+--- Payload ---+
//! | \x00\x0B |  Hello world  | --> | \x00\x0B |  Hello world  |
//! +----------+---------------+     +----------+---------------+
//! ```
//!
//! The value of the length field is 11 (`\x0B`) which represents the length
//! of the payload, `hello world`. By default, [`FramedRead`] assumes that
//! the length field represents the number of bytes that **follows** the
//! length field. Thus, the entire frame has a length of 13: 2 bytes for the
//! frame head + 11 bytes for the payload.
//!
//! ## Example 2
//!
//! The following will parse a `u16` length field at offset 0, omitting the
//! frame head in the yielded `BytesMut`.
//!
//! ```
//! # extern crate tokio;
//! # use tokio::io::AsyncRead;
//! # use tokio::codec::length_delimited;
//! # fn bind_read<T: AsyncRead>(io: T) {
//! length_delimited::Builder::new()
//!     .length_field_offset(0) // default value
//!     .length_field_length(2)
//!     .length_adjustment(0)   // default value
//!     // `num_skip` is not needed, the default is to skip
//!     .new_read(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! The following frame will be decoded as such:
//!
//! ```text
//!          INPUT                        DECODED
//! +-- len ---+--- Payload ---+     +--- Payload ---+
//! | \x00\x0B |  Hello world  | --> |  Hello world  |
//! +----------+---------------+     +---------------+
//! ```
//!
//! This is similar to the first example, the only difference is that the
//! frame head is **not** included in the yielded `BytesMut` value.
//!
//! ## Example 3
//!
//! The following will parse a `u16` length field at offset 0, including the
//! frame head in the yielded `BytesMut`. In this case, the length field
//! **includes** the frame head length.
//!
//! ```
//! # extern crate tokio;
//! # use tokio::io::AsyncRead;
//! # use tokio::codec::length_delimited;
//! # fn bind_read<T: AsyncRead>(io: T) {
//! length_delimited::Builder::new()
//!     .length_field_offset(0) // default value
//!     .length_field_length(2)
//!     .length_adjustment(-2)  // size of head
//!     .num_skip(0)
//!     .new_read(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! The following frame will be decoded as such:
//!
//! ```text
//!          INPUT                           DECODED
//! +-- len ---+--- Payload ---+     +-- len ---+--- Payload ---+
//! | \x00\x0D |  Hello world  | --> | \x00\x0D |  Hello world  |
//! +----------+---------------+     +----------+---------------+
//! ```
//!
//! In most cases, the length field represents the length of the payload
//! only, as shown in the previous examples. However, in some protocols the
//! length field represents the length of the whole frame, including the
//! head. In such cases, we specify a negative `length_adjustment` to adjust
//! the value provided in the frame head to represent the payload length.
//!
//! ## Example 4
//!
//! The following will parse a 3 byte length field at offset 0 in a 5 byte
//! frame head, including the frame head in the yielded `BytesMut`.
//!
//! ```
//! # extern crate tokio;
//! # use tokio::io::AsyncRead;
//! # use tokio::codec::length_delimited;
//! # fn bind_read<T: AsyncRead>(io: T) {
//! length_delimited::Builder::new()
//!     .length_field_offset(0) // default value
//!     .length_field_length(3)
//!     .length_adjustment(2)  // remaining head
//!     .num_skip(0)
//!     .new_read(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! The following frame will be decoded as such:
//!
//! ```text
//!                  INPUT
//! +---- len -----+- head -+--- Payload ---+
//! | \x00\x00\x0B | \xCAFE |  Hello world  |
//! +--------------+--------+---------------+
//!
//!                  DECODED
//! +---- len -----+- head -+--- Payload ---+
//! | \x00\x00\x0B | \xCAFE |  Hello world  |
//! +--------------+--------+---------------+
//! ```
//!
//! A more advanced example that shows a case where there is extra frame
//! head data between the length field and the payload. In such cases, it is
//! usually desirable to include the frame head as part of the yielded
//! `BytesMut`. This lets consumers of the length delimited framer to
//! process the frame head as needed.
//!
//! The positive `length_adjustment` value lets `FramedRead` factor in the
//! additional head into the frame length calculation.
//!
//! ## Example 5
//!
//! The following will parse a `u16` length field at offset 1 of a 4 byte
//! frame head. The first byte and the length field will be omitted from the
//! yielded `BytesMut`, but the trailing 2 bytes of the frame head will be
//! included.
//!
//! ```
//! # extern crate tokio;
//! # use tokio::io::AsyncRead;
//! # use tokio::codec::length_delimited;
//! # fn bind_read<T: AsyncRead>(io: T) {
//! length_delimited::Builder::new()
//!     .length_field_offset(1) // length of hdr1
//!     .length_field_length(2)
//!     .length_adjustment(1)  // length of hdr2
//!     .num_skip(3) // length of hdr1 + LEN
//!     .new_read(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! The following frame will be decoded as such:
//!
//! ```text
//!                  INPUT
//! +- hdr1 -+-- len ---+- hdr2 -+--- Payload ---+
//! |  \xCA  | \x00\x0B |  \xFE  |  Hello world  |
//! +--------+----------+--------+---------------+
//!
//!          DECODED
//! +- hdr2 -+--- Payload ---+
//! |  \xFE  |  Hello world  |
//! +--------+---------------+
//! ```
//!
//! The length field is situated in the middle of the frame head. In this
//! case, the first byte in the frame head could be a version or some other
//! identifier that is not needed for processing. On the other hand, the
//! second half of the head is needed.
//!
//! `length_field_offset` indicates how many bytes to skip before starting
//! to read the length field.  `length_adjustment` is the number of bytes to
//! skip starting at the end of the length field. In this case, it is the
//! second half of the head.
//!
//! ## Example 6
//!
//! The following will parse a `u16` length field at offset 1 of a 4 byte
//! frame head. The first byte and the length field will be omitted from the
//! yielded `BytesMut`, but the trailing 2 bytes of the frame head will be
//! included. In this case, the length field **includes** the frame head
//! length.
//!
//! ```
//! # extern crate tokio;
//! # use tokio::io::AsyncRead;
//! # use tokio::codec::length_delimited;
//! # fn bind_read<T: AsyncRead>(io: T) {
//! length_delimited::Builder::new()
//!     .length_field_offset(1) // length of hdr1
//!     .length_field_length(2)
//!     .length_adjustment(-3)  // length of hdr1 + LEN, negative
//!     .num_skip(3)
//!     .new_read(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! The following frame will be decoded as such:
//!
//! ```text
//!                  INPUT
//! +- hdr1 -+-- len ---+- hdr2 -+--- Payload ---+
//! |  \xCA  | \x00\x0F |  \xFE  |  Hello world  |
//! +--------+----------+--------+---------------+
//!
//!          DECODED
//! +- hdr2 -+--- Payload ---+
//! |  \xFE  |  Hello world  |
//! +--------+---------------+
//! ```
//!
//! Similar to the example above, the difference is that the length field
//! represents the length of the entire frame instead of just the payload.
//! The length of `hdr1` and `len` must be counted in `length_adjustment`.
//! Note that the length of `hdr2` does **not** need to be explicitly set
//! anywhere because it already is factored into the total frame length that
//! is read from the byte stream.
//!
//! # Encoding
//!
//! [`FramedWrite`] adapts an [`AsyncWrite`] into a `Sink` of [`BytesMut`],
//! such that each submitted [`BytesMut`] is prefaced by a length field.
//! There are fewer configuration options than [`FramedRead`]. Given
//! protocols that have more complex frame heads, an encoder should probably
//! be written by hand using [`Encoder`].
//!
//! Here is a simple example, given a `FramedWrite` with the following
//! configuration:
//!
//! ```
//! # extern crate tokio;
//! # extern crate bytes;
//! # use tokio::io::AsyncWrite;
//! # use tokio::codec::length_delimited;
//! # use bytes::BytesMut;
//! # fn write_frame<T: AsyncWrite>(io: T) {
//! # let _ =
//! length_delimited::Builder::new()
//!     .length_field_length(2)
//!     .new_write(io);
//! # }
//! # pub fn main() {}
//! ```
//!
//! A payload of `hello world` will be encoded as:
//!
//! ```text
//! +- len: u16 -+---- data ----+
//! |  \x00\x0b  |  hello world |
//! +------------+--------------+
//! ```
//!
//! [`LengthDelimitedCodec::new()`]: struct.LengthDelimitedCodec.html#method.new
//! [`FramedRead`]: struct.FramedRead.html
//! [`FramedWrite`]: struct.FramedWrite.html
//! [`AsyncRead`]: ../../trait.AsyncRead.html
//! [`AsyncWrite`]: ../../trait.AsyncWrite.html
//! [`Encoder`]: ../trait.Encoder.html
//! [`BytesMut`]: https://docs.rs/bytes/0.4/bytes/struct.BytesMut.html

use {
    codec::{
        Decoder, Encoder, FramedRead, FramedWrite, Framed
    },
    io::{
        AsyncRead, AsyncWrite
    },
};

use bytes::{Buf, BufMut, Bytes, BytesMut, IntoBuf};

use std::{cmp, fmt};
use std::error::Error as StdError;
use std::io::{self, Cursor};

/// Configure length delimited `LengthDelimitedCodec`s.
///
/// `Builder` enables constructing configured length delimited codecs. Note
/// that not all configuration settings apply to both encoding and decoding. See
/// the documentation for specific methods for more detail.
#[derive(Debug, Clone, Copy)]
pub struct Builder {
    // Maximum frame length
    max_frame_len: usize,

    // Number of bytes representing the field length
    length_field_len: usize,

    // Number of bytes in the header before the length field
    length_field_offset: usize,

    // Adjust the length specified in the header field by this amount
    length_adjustment: isize,

    // Total number of bytes to skip before reading the payload, if not set,
    // `length_field_len + length_field_offset`
    num_skip: Option<usize>,

    // Length field byte order (little or big endian)
    length_field_is_big_endian: bool,
}

/// An error when the number of bytes read is more than max frame length.
pub struct FrameTooBig {
    _priv: (),
}

/// A codec for frames delimited by a frame head specifying their lengths.
///
/// This allows the consumer to work with entire frames without having to worry
/// about buffering or other framing logic.
///
/// See [module level] documentation for more detail.
///
/// [module level]: index.html
#[derive(Debug)]
pub struct LengthDelimitedCodec {
    // Configuration values
    builder: Builder,

    // Read state
    state: DecodeState,
}

#[derive(Debug, Clone, Copy)]
enum DecodeState {
    Head,
    Data(usize),
}

// ===== impl LengthDelimitedCodec ======

impl LengthDelimitedCodec {
    /// Creates a new `LengthDelimitedCodec` with the default configuration values.
    pub fn new() -> Self {
        Self {
            builder: Builder::new(),
            state: DecodeState::Head,
        }
    }

    /// Returns the current max frame setting
    ///
    /// This is the largest size this codec will accept from the wire. Larger
    /// frames will be rejected.
    pub fn max_frame_length(&self) -> usize {
        self.builder.max_frame_len
    }

    /// Updates the max frame setting.
    ///
    /// The change takes effect the next time a frame is decoded. In other
    /// words, if a frame is currently in process of being decoded with a frame
    /// size greater than `val` but less than the max frame length in effect
    /// before calling this function, then the frame will be allowed.
    pub fn set_max_frame_length(&mut self, val: usize) {
        self.builder.max_frame_length(val);
    }

    fn decode_head(&mut self, src: &mut BytesMut) -> io::Result<Option<usize>> {
        let head_len = self.builder.num_head_bytes();
        let field_len = self.builder.length_field_len;

        if src.len() < head_len {
            // Not enough data
            return Ok(None);
        }

        let n = {
            let mut src = Cursor::new(&mut *src);

            // Skip the required bytes
            src.advance(self.builder.length_field_offset);

            // match endianess
            let n = if self.builder.length_field_is_big_endian {
                src.get_uint_be(field_len)
            } else {
                src.get_uint_le(field_len)
            };

            if n > self.builder.max_frame_len as u64 {
                return Err(io::Error::new(io::ErrorKind::InvalidData, FrameTooBig {
                    _priv: (),
                }));
            }

            // The check above ensures there is no overflow
            let n = n as usize;

            // Adjust `n` with bounds checking
            let n = if self.builder.length_adjustment < 0 {
                n.checked_sub(-self.builder.length_adjustment as usize)
            } else {
                n.checked_add(self.builder.length_adjustment as usize)
            };

            // Error handling
            match n {
                Some(n) => n,
                None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "provided length would overflow after adjustment")),
            }
        };

        let num_skip = self.builder.get_num_skip();

        if num_skip > 0 {
            let _ = src.split_to(num_skip);
        }

        // Ensure that the buffer has enough space to read the incoming
        // payload
        src.reserve(n);

        return Ok(Some(n));
    }

    fn decode_data(&self, n: usize, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        // At this point, the buffer has already had the required capacity
        // reserved. All there is to do is read.
        if src.len() < n {
            return Ok(None);
        }

        Ok(Some(src.split_to(n)))
    }
}

impl Decoder for LengthDelimitedCodec {
    type Item = BytesMut;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> io::Result<Option<BytesMut>> {
        let n = match self.state {
            DecodeState::Head => {
                match try!(self.decode_head(src)) {
                    Some(n) => {
                        self.state = DecodeState::Data(n);
                        n
                    }
                    None => return Ok(None),
                }
            }
            DecodeState::Data(n) => n,
        };

        match try!(self.decode_data(n, src)) {
            Some(data) => {
                // Update the decode state
                self.state = DecodeState::Head;

                // Make sure the buffer has enough space to read the next head
                src.reserve(self.builder.num_head_bytes());

                Ok(Some(data))
            }
            None => Ok(None),
        }
    }
}

impl Encoder for LengthDelimitedCodec {
    type Item = Bytes;
    type Error = io::Error;

    fn encode(&mut self, data: Bytes, dst: &mut BytesMut) -> Result<(), io::Error> {
        let n = (&data).into_buf().remaining();

        if n > self.builder.max_frame_len {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, FrameTooBig {
                _priv: (),
            }));
        }

        // Adjust `n` with bounds checking
        let n = if self.builder.length_adjustment < 0 {
            n.checked_add(-self.builder.length_adjustment as usize)
        } else {
            n.checked_sub(self.builder.length_adjustment as usize)
        };

        let n = n.ok_or_else(|| io::Error::new(
            io::ErrorKind::InvalidInput,
            "provided length would overflow after adjustment",
        ))?;

        // Reserve capacity in the destination buffer to fit the frame and
        // length field (plus adjustment).
        dst.reserve(self.builder.length_field_len + n);

        if self.builder.length_field_is_big_endian {
            dst.put_uint_be(n as u64, self.builder.length_field_len);
        } else {
            dst.put_uint_le(n as u64, self.builder.length_field_len);
        }

        // Write the frame to the buffer
        dst.extend_from_slice(&data[..]);

        Ok(())
    }
}

// ===== impl Builder =====

impl Builder {
    /// Creates a new length delimited codec builder with default configuration
    /// values.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .length_field_offset(0)
    ///     .length_field_length(2)
    ///     .length_adjustment(0)
    ///     .num_skip(0)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn new() -> Builder {
        Builder {
            // Default max frame length of 8MB
            max_frame_len: 8 * 1_024 * 1_024,

            // Default byte length of 4
            length_field_len: 4,

            // Default to the header field being at the start of the header.
            length_field_offset: 0,

            length_adjustment: 0,

            // Total number of bytes to skip before reading the payload, if not set,
            // `length_field_len + length_field_offset`
            num_skip: None,

            // Default to reading the length field in network (big) endian.
            length_field_is_big_endian: true,
        }
    }

    /// Read the length field as a big endian integer
    ///
    /// This is the default setting.
    ///
    /// This configuration option applies to both encoding and decoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .big_endian()
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn big_endian(&mut self) -> &mut Self {
        self.length_field_is_big_endian = true;
        self
    }

    /// Read the length field as a little endian integer
    ///
    /// The default setting is big endian.
    ///
    /// This configuration option applies to both encoding and decoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .little_endian()
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn little_endian(&mut self) -> &mut Self {
        self.length_field_is_big_endian = false;
        self
    }

    /// Read the length field as a native endian integer
    ///
    /// The default setting is big endian.
    ///
    /// This configuration option applies to both encoding and decoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .native_endian()
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn native_endian(&mut self) -> &mut Self {
        if cfg!(target_endian = "big") {
            self.big_endian()
        } else {
            self.little_endian()
        }
    }

    /// Sets the max frame length
    ///
    /// This configuration option applies to both encoding and decoding. The
    /// default value is 8MB.
    ///
    /// When decoding, the length field read from the byte stream is checked
    /// against this setting **before** any adjustments are applied. When
    /// encoding, the length of the submitted payload is checked against this
    /// setting.
    ///
    /// When frames exceed the max length, an `io::Error` with the custom value
    /// of the `FrameTooBig` type will be returned.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .max_frame_length(8 * 1024)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn max_frame_length(&mut self, val: usize) -> &mut Self {
        self.max_frame_len = val;
        self
    }

    /// Sets the number of bytes used to represent the length field
    ///
    /// The default value is `4`. The max value is `8`.
    ///
    /// This configuration option applies to both encoding and decoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .length_field_length(4)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn length_field_length(&mut self, val: usize) -> &mut Self {
        assert!(val > 0 && val <= 8, "invalid length field length");
        self.length_field_len = val;
        self
    }

    /// Sets the number of bytes in the header before the length field
    ///
    /// This configuration option only applies to decoding.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .length_field_offset(1)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn length_field_offset(&mut self, val: usize) -> &mut Self {
        self.length_field_offset = val;
        self
    }

    /// Delta between the payload length specified in the header and the real
    /// payload length
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .length_adjustment(-2)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn length_adjustment(&mut self, val: isize) -> &mut Self {
        self.length_adjustment = val;
        self
    }

    /// Sets the number of bytes to skip before reading the payload
    ///
    /// Default value is `length_field_len + length_field_offset`
    ///
    /// This configuration option only applies to decoding
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .num_skip(4)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn num_skip(&mut self, val: usize) -> &mut Self {
        self.num_skip = Some(val);
        self
    }

    /// Create a configured length delimited `LengthDelimitedCodec`
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    /// # pub fn main() {
    /// Builder::new()
    ///     .length_field_offset(0)
    ///     .length_field_length(2)
    ///     .length_adjustment(0)
    ///     .num_skip(0)
    ///     .new_codec();
    /// # }
    /// ```
    pub fn new_codec(&self) -> LengthDelimitedCodec {
        LengthDelimitedCodec {
            builder: *self,
            state: DecodeState::Head,
        }
    }

    /// Create a configured length delimited `FramedRead`
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # use tokio::io::AsyncRead;
    /// use tokio::codec::length_delimited::Builder;
    ///
    /// # fn bind_read<T: AsyncRead>(io: T) {
    /// Builder::new()
    ///     .length_field_offset(0)
    ///     .length_field_length(2)
    ///     .length_adjustment(0)
    ///     .num_skip(0)
    ///     .new_read(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn new_read<T>(&self, upstream: T) -> FramedRead<T, LengthDelimitedCodec>
        where T: AsyncRead,
    {
        FramedRead::new(upstream, self.new_codec())
    }

    /// Create a configured length delimited `FramedWrite`
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate bytes;
    /// # use tokio::io::AsyncWrite;
    /// # use tokio::codec::length_delimited;
    /// # use bytes::BytesMut;
    /// # fn write_frame<T: AsyncWrite>(io: T) {
    /// length_delimited::Builder::new()
    ///     .length_field_length(2)
    ///     .new_write(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn new_write<T>(&self, inner: T) -> FramedWrite<T, LengthDelimitedCodec>
        where T: AsyncWrite,
    {
        FramedWrite::new(inner, self.new_codec())
    }

    /// Create a configured length delimited `Framed`
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate tokio;
    /// # extern crate bytes;
    /// # use tokio::io::{AsyncRead, AsyncWrite};
    /// # use tokio::codec::length_delimited;
    /// # use bytes::BytesMut;
    /// # fn write_frame<T: AsyncRead + AsyncWrite>(io: T) {
    /// # let _ =
    /// length_delimited::Builder::new()
    ///     .length_field_length(2)
    ///     .new_framed(io);
    /// # }
    /// # pub fn main() {}
    /// ```
    pub fn new_framed<T>(&self, inner: T) -> Framed<T, LengthDelimitedCodec>
        where T: AsyncRead + AsyncWrite,
    {
        Framed::new(inner, self.new_codec())
    }

    fn num_head_bytes(&self) -> usize {
        let num = self.length_field_offset + self.length_field_len;
        cmp::max(num, self.num_skip.unwrap_or(0))
    }

    fn get_num_skip(&self) -> usize {
        self.num_skip.unwrap_or(self.length_field_offset + self.length_field_len)
    }
}


// ===== impl FrameTooBig =====

impl fmt::Debug for FrameTooBig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("FrameTooBig")
            .finish()
    }
}

impl fmt::Display for FrameTooBig {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(self.description())
    }
}

impl StdError for FrameTooBig {
    fn description(&self) -> &str {
        "frame size too big"
    }
}
