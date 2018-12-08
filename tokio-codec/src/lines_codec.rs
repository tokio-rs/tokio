use bytes::{BufMut, BytesMut};
use tokio_io::_tokio_codec::{Encoder, Decoder};
use std::{cmp, io, str, usize};

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

    /// The maximum length for a given line. If `usize::MAX`, lines will be
    /// read until a `\n` character is reached.
    max_length: usize,

    /// Are we currently discarding the remainder of a line which was over
    /// the length limit?
    is_discarding: bool,
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
            max_length: usize::MAX,
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
    pub fn new_with_max_length(max_length: usize) -> Self {
        LinesCodec {
            max_length,
            ..LinesCodec::new()
        }
    }

    /// Returns the maximum line length when decoding.
    ///
    /// ```
    /// use std::usize;
    /// use tokio_codec::LinesCodec;
    ///
    /// let codec = LinesCodec::new();
    /// assert_eq!(codec.max_length(), usize::MAX);
    /// ```
    /// ```
    /// use tokio_codec::LinesCodec;
    ///
    /// let codec = LinesCodec::new_with_max_length(256);
    /// assert_eq!(codec.max_length(), 256);
    /// ```
    pub fn max_length(&self) -> usize {
        self.max_length
    }

    fn discard(&mut self, newline_offset: Option<usize>, read_to: usize, buf: &mut BytesMut) {
        let discard_to = if let Some(offset) = newline_offset {
            // If we found a newline, discard up to that offset and
            // then stop discarding. On the next iteration, we'll try
            // to read a line normally.
            self.is_discarding = false;
            offset + self.next_index + 1
        } else {
            // Otherwise, we didn't find a newline, so we'll discard
            // everything we read. On the next iteration, we'll continue
            // discarding up to max_len bytes unless we find a newline.
            read_to
        };
        buf.advance(discard_to);
        self.next_index = 0;
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
    // TODO: in the next breaking change, this should be changed to a custom
    // error type that indicates the "max length exceeded" condition better.
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<String>, io::Error> {
        loop {
            // Determine how far into the buffer we'll search for a newline. If
            // there's no max_length set, we'll read to the end of the buffer.
            let read_to = cmp::min(self.max_length.saturating_add(1), buf.len());

            let newline_offset = buf[self.next_index..read_to]
                .iter()
                .position(|b| *b == b'\n');

            if self.is_discarding {
                self.discard(newline_offset, read_to, buf);
            } else {
                return if let Some(offset) = newline_offset {
                    // Found a line!
                    let newline_index = offset + self.next_index;
                    self.next_index = 0;
                    let line = buf.split_to(newline_index + 1);
                    let line = &line[..line.len() - 1];
                    let line = without_carriage_return(line);
                    let line = utf8(line)?;

                    Ok(Some(line.to_string()))
                } else if buf.len() > self.max_length {
                    // Reached the maximum length without finding a
                    // newline, return an error and start discarding on the
                    // next call.
                    self.is_discarding = true;
                    Err(io::Error::new(
                        io::ErrorKind::Other,
                        "line length limit exceeded"
                    ))
                } else {
                    // We didn't find a line or reach the length limit, so the next
                    // call will resume searching at the current offset.
                    self.next_index = read_to;
                    Ok(None)
                };
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
