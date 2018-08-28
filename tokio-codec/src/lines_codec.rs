use bytes::{BufMut, BytesMut};
use tokio_io::_tokio_codec::{Encoder, Decoder};
use std::{io, str};

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
}

impl LinesCodec {
    /// Returns a `LinesCodec` for splitting up data into lines.
    pub fn new() -> LinesCodec {
        LinesCodec {
            next_index: 0,
            max_length: None,
        }
    }

    /// Sets a limit on the maximum line length when decoding.
    ///
    /// If this is set, lines will be ended when a `\n` character is read, _or_
    /// when they reach the provided number of bytes. Otherwise, lines will
    /// only be ended when a `\n` character is read.
    pub fn set_decode_max_line_length(&mut self, limit: usize) -> &mut Self {
        self.max_length = Some(limit);
        self
    }

    /// Returns the current maximum line length when decoding, if one is set.
    ///
    /// ```
    /// use tokio_codec::LinesCodec;
    ///
    /// let mut codec = LinesCodec::new();
    /// assert_eq!(codec.decode_max_line_length(), None);
    ///
    /// codec.set_decode_max_line_length(256);
    /// assert_eq!(codec.decode_max_line_length(), Some(256));
    /// ```
    pub fn decode_max_line_length(&self) -> Option<usize> {
        self.max_length
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
        for (offset, b) in buf[self.next_index..].iter().enumerate() {
           trim_and_offset = match (b, self.max_length) {
                // The current character is a newline, split here.
                (b'\n', _) => Some((1, offset)),
                // There's a maximum line length set, and we've reached it.
                (_, Some(max_len)) if offset == max_len =>
                    // If we're at the line length limit, check if the next
                    // character(s) is a newline --- if so, slice that off
                    // as well, so that the next call to `decode` doesn't
                    // return an empty line.
                    match buf[offset + 1] {
                        b'\r' if buf[offset + 2] == b'\n' =>
                            Some((2, offset + 2)),
                        b'\n' => Some((1, offset + 1)),
                        _ => Some((0, offset)),
                    },
                // The current character isn't a newline, and we aren't at the
                // length limit, so keep going.
                _ => continue,
            };
            break;
        };
        if let Some((trim_amt, offset)) = trim_and_offset {
            let newline_index = offset + self.next_index;
            let line = buf.split_to(newline_index + 1);
            let line = &line[..line.len() - trim_amt];
            let line = without_carriage_return(line);
            let line = utf8(line)?;
            self.next_index = 0;
            Ok(Some(line.to_string()))
        } else {
            self.next_index = buf.len();
            Ok(None)
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
