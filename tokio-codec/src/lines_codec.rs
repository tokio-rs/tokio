use bytes::{BufMut, BytesMut};
use tokio_io::_tokio_codec::{Encoder, Decoder};
use std::{error, fmt, io, str};

/// A simple `Codec` implementation that splits up data into lines.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct LinesCodec {
    // Stored index of the next index to examine for a `\n` character.
    // This is used to optimize searching.
    // For example, if `decode` was called with `abc`, it would hold `3`,
    // because that is the next index to examine.
    // The next time `decode` is called with `abcde\n`, the method will
    // only look at `de\n` before returning.
    next_index: usize,

    /// The maximum length for a given line. If `None`, lines will be read
    /// until a `\n` character is reached.
    max_length: Option<usize>,

    /// Are we currently discarding the remainder of a line which was over
    /// the length limit?
    is_discarding: bool,
}

/// Error indicating that the maximum line length was reached.
#[derive(Debug)]
pub struct LengthError {
    limit: usize,
}

impl LinesCodec {
    /// Returns a `LinesCodec` for splitting up data into lines.
    ///
    /// # Note
    ///
    /// The returned `LinesCodec` will not have an upper bound on the length
    /// of a buffered line. See the documentation for [`new_with_max_length`]
    /// for information on why this could be a potential security risk.
    ///
    /// [`new_with_max_length`]: #method.new_with_max_length
    pub fn new() -> LinesCodec {
        LinesCodec {
            next_index: 0,
            max_length: None,
            is_discarding: false,
        }
    }

    /// Returns a `LinesCodec` with a maximum line length limit.
    ///
    /// If this is set, calls to `LinesCodec::decode` will return a
    /// [`LengthError`] when a line exceeds the length limit. Subsequent calls
    /// will discard up to `limit` bytes from that line until a newline
    /// character is reached, returning `None` until the line over the limit
    /// has been fully discarded. After that point, calls to `decode` will
    /// function as normal.
    ///
    /// # Note
    ///
    /// Setting a length limit is highly recommended for any `LinesCodec` which
    /// will be exposed to untrusted input. Otherwise, the size of the buffer
    /// that holds the line currently being read is unbounded. An attacker could
    /// exploit this unbounded buffer by sending an unbounded amount of input
    /// without any `\n` characters, causing unbounded memory consumption.
    ///
    /// [`LengthError`]: ../struct.LengthError
    pub fn new_with_max_length(limit: usize) -> Self {
        LinesCodec {
            max_length: Some(limit - 1),
            ..LinesCodec::new()
        }
    }

    /// Returns the current maximum line length when decoding, if one is set.
    ///
    /// ```
    /// use tokio_codec::LinesCodec;
    ///
    /// let codec = LinesCodec::new();
    /// assert_eq!(codec.max_length(), None);
    /// ```
    /// ```
    /// use tokio_codec::LinesCodec;
    ///
    /// let codec = LinesCodec::new_with_max_length(256);
    /// assert_eq!(codec.max_length(), Some(256));
    /// ```
    pub fn max_length(&self) -> Option<usize> {
        self.max_length.map(|len| len + 1)
    }
}

fn utf8(buf: &[u8]) -> Result<&str, io::Error> {
    str::from_utf8(buf).map_err(|_|
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Unable to decode input as UTF8"))
}

fn without_carriage_return(s: &[u8]) -> &[u8] {
    if let Some(&b'\r') = s.last() {
        &s[..s.len() - 1]
    } else {
        s
    }
}

impl Decoder for LinesCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, io::Error> {
        let mut trim_and_offset = None;
        loop {
            'inner: for (offset, b) in buf[self.next_index..].iter().enumerate() {
                let should_discard = self.is_discarding;
                trim_and_offset = match (b, self.max_length) {
                    // The current character is a newline, split here.
                    (&b'\n', _) => {
                        // Stop discarding on subsequent reads.
                        self.is_discarding = false;
                        Some((1, offset, should_discard))
                    },
                    // There's a maximum line length set, and we've reached it.
                    (_, Some(max_len))
                        if offset.saturating_sub(self.next_index) == max_len => {
                        // If we're at the line length limit, check if the next
                        // character(s) is a newline before we decide to return an
                        // error.
                        match buf.get(offset + 1) {
                            Some(&b'\n')  => {
                                // Found a newline, stop discarding.
                                self.is_discarding = false;
                                Some((1, offset + 1, should_discard))
                            },
                            None =>
                                // We are at the length limit exactly, but we are
                                // at the end of the buffer. The next character to
                                // be added to the buffer may be a newline, so just
                                // return `None` here rather than an error, and try
                                // again if `decode` is called again.
                                return Ok(None),
                            Some(_) => {
                                // We've reached the length limit, and we're not at
                                // the end of a line. Subsequent calls to decode
                                // will now discard from the buffer until we reach
                                // a new line.
                                self.is_discarding = true;
                                Some((0, offset, true))
                            }
                        }
                    },
                    // The current character isn't a newline, and we aren't at the
                    // length limit, so keep going.
                    _ => continue 'inner,
                };
                break 'inner;
            };
            match trim_and_offset {
                Some((_, offset, true)) => {
                    let discard_index = offset + self.next_index;
                    self.next_index = 0;
                    buf.advance(discard_index + 1);
                    if !self.is_discarding {
                        let limit = self.max_length
                            .expect("max length should be set if discarding");
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            LengthError { limit: limit + 1 },
                        ));
                    }
                },
                Some((trim_amt, offset, false)) => {
                    let newline_index = offset + self.next_index;
                    self.next_index = 0;
                    let line = buf.split_to(newline_index + 1);
                    let line = &line[..line.len() - trim_amt];
                    let line = without_carriage_return(line);
                    let line = utf8(line)?;

                    return Ok(Some(line.to_string()))
                },
                None => {
                    self.next_index = buf.len();
                    return Ok(None)
                },
            }
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<String>, io::Error> {
        Ok(match self.decode(buf)? {
            Some(frame) => Some(frame),
            None => {
                // No terminating newline - return remaining data, if any
                if buf.is_empty() || buf == &b"\r"[..] {
                    None
                } else {
                    let line = buf.take();
                    let line = without_carriage_return(&line);
                    let line = utf8(line)?;
                    self.next_index = 0;
                    Some(line.to_string())
                }
            }
        })
    }
}

impl Encoder for LinesCodec {
    type Item = String;
    type Error = io::Error;

    fn encode(&mut self, line: String, buf: &mut BytesMut) -> Result<(), io::Error> {
        buf.reserve(line.len() + 1);
        buf.put(line);
        buf.put_u8(b'\n');
        Ok(())
    }
}

impl fmt::Display for LengthError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "reached maximum line length ({} characters)", self.limit)
    }
}

impl error::Error for LengthError {
    fn description(&self) -> &str {
        "reached maximum line length"
    }

    fn cause(&self) -> Option<&error::Error> {
        Some(self)
    }
}
