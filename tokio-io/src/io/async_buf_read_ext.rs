use crate::io::lines::{lines, Lines};
use crate::io::read_line::{read_line, ReadLine};
use crate::io::read_until::{read_until, ReadUntil};
use crate::io::split::{split, Split};
use crate::AsyncBufRead;

/// An extension trait which adds utility methods to `AsyncBufRead` types.
pub trait AsyncBufReadExt: AsyncBufRead {
    /// Creates a future which will read all the bytes associated with this I/O
    /// object into `buf` until the delimiter `byte` or EOF is reached.
    /// This method is the async equivalent to [`BufRead::read_until`](std::io::BufRead::read_until).
    ///
    /// This function will read bytes from the underlying stream until the
    /// delimiter or EOF is found. Once found, all bytes up to, and including,
    /// the delimiter (if found) will be appended to `buf`.
    ///
    /// The returned future will resolve to the number of bytes read once the read
    /// operation is completed.
    ///
    /// In the case of an error the buffer and the object will be discarded, with
    /// the error yielded.
    fn read_until<'a>(&'a mut self, byte: u8, buf: &'a mut Vec<u8>) -> ReadUntil<'a, Self>
    where
        Self: Unpin,
    {
        read_until(self, byte, buf)
    }

    /// Creates a future which will read all the bytes associated with this I/O
    /// object into `buf` until a newline (the 0xA byte) or EOF is reached,
    /// This method is the async equivalent to [`BufRead::read_line`](std::io::BufRead::read_line).
    ///
    /// This function will read bytes from the underlying stream until the
    /// newline delimiter (the 0xA byte) or EOF is found. Once found, all bytes
    /// up to, and including, the delimiter (if found) will be appended to
    /// `buf`.
    ///
    /// The returned future will resolve to the number of bytes read once the read
    /// operation is completed.
    ///
    /// In the case of an error the buffer and the object will be discarded, with
    /// the error yielded.
    ///
    /// # Errors
    ///
    /// This function has the same error semantics as [`read_until`] and will
    /// also return an error if the read bytes are not valid UTF-8. If an I/O
    /// error is encountered then `buf` may contain some bytes already read in
    /// the event that all data read so far was valid UTF-8.
    ///
    /// [`read_until`]: AsyncBufReadExt::read_until
    fn read_line<'a>(&'a mut self, buf: &'a mut String) -> ReadLine<'a, Self>
    where
        Self: Unpin,
    {
        read_line(self, buf)
    }

    /// Returns a stream of the contents of this reader split on the byte
    /// `byte`.
    ///
    /// This method is the async equivalent to
    /// [`BufRead::split`](std::io::BufRead::split).
    ///
    /// The stream returned from this function will yield instances of
    /// [`io::Result`]`<`[`Vec<u8>`]`>`. Each vector returned will *not* have
    /// the delimiter byte at the end.
    ///
    /// [`io::Result`]: std::io::Result
    /// [`Vec<u8>`]: std::vec::Vec
    ///
    /// # Errors
    ///
    /// Each item of the stream has the same error semantics as
    /// [`AsyncBufReadExt::read_until`](AsyncBufReadExt::read_until).
    fn split(self, byte: u8) -> Split<Self>
    where
        Self: Sized,
    {
        split(self, byte)
    }

    /// Returns a stream over the lines of this reader.
    /// This method is the async equivalent to [`BufRead::lines`](std::io::BufRead::lines).
    ///
    /// The stream returned from this function will yield instances of
    /// [`io::Result`]`<`[`String`]`>`. Each string returned will *not* have a newline
    /// byte (the 0xA byte) or CRLF (0xD, 0xA bytes) at the end.
    ///
    /// [`io::Result`]: std::io::Result
    /// [`String`]: String
    ///
    /// # Errors
    ///
    /// Each line of the stream has the same error semantics as [`AsyncBufReadExt::read_line`].
    ///
    /// [`AsyncBufReadExt::read_line`]: AsyncBufReadExt::read_line
    fn lines(self) -> Lines<Self>
    where
        Self: Sized,
    {
        lines(self)
    }
}

impl<R: AsyncBufRead + ?Sized> AsyncBufReadExt for R {}
