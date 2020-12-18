use super::AsyncRead;
use super::read_until::{read_until, ReadUntil};
use super::read_line::{read_line, ReadLine};
use super::lines::{lines, Lines};
use super::util::split::{split, Split};

use std::io;
use std::ops::DerefMut;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Reads bytes asynchronously.
///
/// This trait is analogous to [`std::io::BufRead`], but integrates with
/// the asynchronous task system. In particular, the [`poll_fill_buf`] method,
/// unlike [`BufRead::fill_buf`], will automatically queue the current task for wakeup
/// and return if data is not yet available, rather than blocking the calling
/// thread.
///
/// Utilities for working with `AsyncBufRead` values are provided by
/// [`AsyncBufRead`].
///
/// [`std::io::BufRead`]: std::io::BufRead
/// [`poll_fill_buf`]: AsyncBufRead::poll_fill_buf
/// [`BufRead::fill_buf`]: std::io::BufRead::fill_buf
/// [`AsyncBufRead`]: crate::io::AsyncBufRead
pub trait AsyncBufRead: AsyncRead {
    /// Attempts to return the contents of the internal buffer, filling it with more data
    /// from the inner reader if it is empty.
    ///
    /// On success, returns `Poll::Ready(Ok(buf))`.
    ///
    /// If no data is available for reading, the method returns
    /// `Poll::Pending` and arranges for the current task (via
    /// `cx.waker().wake_by_ref()`) to receive a notification when the object becomes
    /// readable or is closed.
    ///
    /// This function is a lower-level call. It needs to be paired with the
    /// [`consume`] method to function properly. When calling this
    /// method, none of the contents will be "read" in the sense that later
    /// calling [`poll_read`] may return the same contents. As such, [`consume`] must
    /// be called with the number of bytes that are consumed from this buffer to
    /// ensure that the bytes are never returned twice.
    ///
    /// An empty buffer returned indicates that the stream has reached EOF.
    ///
    /// [`poll_read`]: AsyncRead::poll_read
    /// [`consume`]: AsyncBufRead::consume
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>>;

    /// Tells this buffer that `amt` bytes have been consumed from the buffer,
    /// so they should no longer be returned in calls to [`poll_read`].
    ///
    /// This function is a lower-level call. It needs to be paired with the
    /// [`poll_fill_buf`] method to function properly. This function does
    /// not perform any I/O, it simply informs this object that some amount of
    /// its buffer, returned from [`poll_fill_buf`], has been consumed and should
    /// no longer be returned. As such, this function may do odd things if
    /// [`poll_fill_buf`] isn't called before calling it.
    ///
    /// The `amt` must be `<=` the number of bytes in the buffer returned by
    /// [`poll_fill_buf`].
    ///
    /// [`poll_read`]: AsyncRead::poll_read
    /// [`poll_fill_buf`]: AsyncBufRead::poll_fill_buf
    fn consume(self: Pin<&mut Self>, amt: usize);

    /// Reads all bytes into `buf` until the delimiter `byte` or EOF is reached.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> io::Result<usize>;
    /// ```
    ///
    /// This function will read bytes from the underlying stream until the
    /// delimiter or EOF is found. Once found, all bytes up to, and including,
    /// the delimiter (if found) will be appended to `buf`.
    ///
    /// If successful, this function will return the total number of bytes read.
    ///
    /// # Errors
    ///
    /// This function will ignore all instances of [`ErrorKind::Interrupted`] and
    /// will otherwise return any errors returned by [`fill_buf`].
    ///
    /// If an I/O error is encountered then all bytes read so far will be
    /// present in `buf` and its length will have been adjusted appropriately.
    ///
    /// [`fill_buf`]: AsyncBufRead::poll_fill_buf
    /// [`ErrorKind::Interrupted`]: std::io::ErrorKind::Interrupted
    ///
    /// # Examples
    ///
    /// [`std::io::Cursor`][`Cursor`] is a type that implements `BufRead`. In
    /// this example, we use [`Cursor`] to read all the bytes in a byte slice
    /// in hyphen delimited segments:
    ///
    /// [`Cursor`]: std::io::Cursor
    ///
    /// ```
    /// use tokio::io::AsyncBufRead;
    ///
    /// use std::io::Cursor;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut cursor = Cursor::new(b"lorem-ipsum");
    ///     let mut buf = vec![];
    ///
    ///     // cursor is at 'l'
    ///     let num_bytes = cursor.read_until(b'-', &mut buf)
    ///         .await
    ///         .expect("reading from cursor won't fail");
    ///
    ///     assert_eq!(num_bytes, 6);
    ///     assert_eq!(buf, b"lorem-");
    ///     buf.clear();
    ///
    ///     // cursor is at 'i'
    ///     let num_bytes = cursor.read_until(b'-', &mut buf)
    ///         .await
    ///         .expect("reading from cursor won't fail");
    ///
    ///     assert_eq!(num_bytes, 5);
    ///     assert_eq!(buf, b"ipsum");
    ///     buf.clear();
    ///
    ///     // cursor is at EOF
    ///     let num_bytes = cursor.read_until(b'-', &mut buf)
    ///         .await
    ///         .expect("reading from cursor won't fail");
    ///     assert_eq!(num_bytes, 0);
    ///     assert_eq!(buf, b"");
    /// }
    /// ```
    fn read_until<'a>(&'a mut self, byte: u8, buf: &'a mut Vec<u8>) -> ReadUntil<'a, Self>
    where
        Self: Sized + Unpin,
    {
        read_until(self, byte, buf)
    }

    /// Reads all bytes until a newline (the 0xA byte) is reached, and append
    /// them to the provided buffer.
    ///
    /// Equivalent to:
    ///
    /// ```ignore
    /// async fn read_line(&mut self, buf: &mut String) -> io::Result<usize>;
    /// ```
    ///
    /// This function will read bytes from the underlying stream until the
    /// newline delimiter (the 0xA byte) or EOF is found. Once found, all bytes
    /// up to, and including, the delimiter (if found) will be appended to
    /// `buf`.
    ///
    /// If successful, this function will return the total number of bytes read.
    ///
    /// If this function returns `Ok(0)`, the stream has reached EOF.
    ///
    /// # Errors
    ///
    /// This function has the same error semantics as [`read_until`] and will
    /// also return an error if the read bytes are not valid UTF-8. If an I/O
    /// error is encountered then `buf` may contain some bytes already read in
    /// the event that all data read so far was valid UTF-8.
    ///
    /// [`read_until`]: AsyncBufRead::read_until
    ///
    /// # Examples
    ///
    /// [`std::io::Cursor`][`Cursor`] is a type that implements
    /// `AsyncBufRead`. In this example, we use [`Cursor`] to read all the
    /// lines in a byte slice:
    ///
    /// [`Cursor`]: std::io::Cursor
    ///
    /// ```
    /// use tokio::io::AsyncBufRead;
    ///
    /// use std::io::Cursor;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut cursor = Cursor::new(b"foo\nbar");
    ///     let mut buf = String::new();
    ///
    ///     // cursor is at 'f'
    ///     let num_bytes = cursor.read_line(&mut buf)
    ///         .await
    ///         .expect("reading from cursor won't fail");
    ///
    ///     assert_eq!(num_bytes, 4);
    ///     assert_eq!(buf, "foo\n");
    ///     buf.clear();
    ///
    ///     // cursor is at 'b'
    ///     let num_bytes = cursor.read_line(&mut buf)
    ///         .await
    ///         .expect("reading from cursor won't fail");
    ///
    ///     assert_eq!(num_bytes, 3);
    ///     assert_eq!(buf, "bar");
    ///     buf.clear();
    ///
    ///     // cursor is at EOF
    ///     let num_bytes = cursor.read_line(&mut buf)
    ///         .await
    ///         .expect("reading from cursor won't fail");
    ///
    ///     assert_eq!(num_bytes, 0);
    ///     assert_eq!(buf, "");
    /// }
    /// ```
    fn read_line<'a>(&'a mut self, buf: &'a mut String) -> ReadLine<'a, Self>
    where
        Self: Sized + Unpin,
    {
        read_line(self, buf)
    }

    /// Returns a stream of the contents of this reader split on the byte
    /// `byte`.
    ///
    /// This method is the asynchronous equivalent to
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
    /// [`AsyncBufRead::read_until`](AsyncBufRead::read_until).
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::AsyncBufRead;
    ///
    /// # async fn dox(my_buf_read: impl AsyncBufRead + Unpin) -> std::io::Result<()> {
    /// let mut segments = my_buf_read.split(b'f');
    ///
    /// while let Some(segment) = segments.next_segment().await? {
    ///     println!("length = {}", segment.len())
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn split(self, byte: u8) -> Split<Self>
    where
        Self: Sized + Unpin,
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
    /// Each line of the stream has the same error semantics as [`AsyncBufRead::read_line`].
    ///
    /// # Examples
    ///
    /// [`std::io::Cursor`][`Cursor`] is a type that implements `BufRead`. In
    /// this example, we use [`Cursor`] to iterate over all the lines in a byte
    /// slice.
    ///
    /// [`Cursor`]: std::io::Cursor
    ///
    /// ```
    /// use tokio::io::AsyncBufRead;
    ///
    /// use std::io::Cursor;
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let cursor = Cursor::new(b"lorem\nipsum\r\ndolor");
    ///
    ///     let mut lines = cursor.lines();
    ///
    ///     assert_eq!(lines.next_line().await.unwrap(), Some(String::from("lorem")));
    ///     assert_eq!(lines.next_line().await.unwrap(), Some(String::from("ipsum")));
    ///     assert_eq!(lines.next_line().await.unwrap(), Some(String::from("dolor")));
    ///     assert_eq!(lines.next_line().await.unwrap(), None);
    /// }
    /// ```
    ///
    /// [`AsyncBufRead::read_line`]: AsyncBufRead::read_line
    fn lines(self) -> Lines<Self>
    where
        Self: Sized,
    {
        lines(self)
    }
}

macro_rules! deref_async_buf_read {
    () => {
        fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
            Pin::new(&mut **self.get_mut()).poll_fill_buf(cx)
        }

        fn consume(mut self: Pin<&mut Self>, amt: usize) {
            Pin::new(&mut **self).consume(amt)
        }
    };
}

impl<T: ?Sized + AsyncBufRead + Unpin> AsyncBufRead for Box<T> {
    deref_async_buf_read!();
}

impl<T: ?Sized + AsyncBufRead + Unpin> AsyncBufRead for &mut T {
    deref_async_buf_read!();
}

impl<P> AsyncBufRead for Pin<P>
where
    P: DerefMut + Unpin,
    P::Target: AsyncBufRead,
{
    fn poll_fill_buf(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        self.get_mut().as_mut().poll_fill_buf(cx)
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        self.get_mut().as_mut().consume(amt)
    }
}

impl AsyncBufRead for &[u8] {
    fn poll_fill_buf(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        Poll::Ready(Ok(*self))
    }

    fn consume(mut self: Pin<&mut Self>, amt: usize) {
        *self = &self[amt..];
    }
}

impl<T: AsRef<[u8]> + Unpin> AsyncBufRead for io::Cursor<T> {
    fn poll_fill_buf(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<&[u8]>> {
        Poll::Ready(io::BufRead::fill_buf(self.get_mut()))
    }

    fn consume(self: Pin<&mut Self>, amt: usize) {
        io::BufRead::consume(self.get_mut(), amt)
    }
}
