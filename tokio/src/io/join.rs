//! Join two values implementing `AsyncRead` and `AsyncWrite` into a single one.

use crate::io::{AsyncRead, AsyncWrite, ReadBuf};

use std::fmt;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

cfg_io_util! {
    pin_project_lite::pin_project! {
        /// Joins two values implementing `AsyncRead` and `AsyncWrite` into a
        /// single handle.
        pub struct Join<R, W> {
            #[pin]
            reader: R,
            #[pin]
            writer: W,
        }
    }

    impl<R, W> Join<R, W>
    where
        R: AsyncRead + Unpin,
        W: AsyncWrite + Unpin,
    {
        /// Join two values implementing `AsyncRead` and `AsyncWrite` into a
        /// single handle.
        pub fn new(reader: R, writer: W) -> Self {
            Self { reader, writer }
        }

        /// Splits this `Join` back into its `AsyncRead` and `AsyncWrite`
        /// components.
        pub fn split(self) -> (R, W) {
            (self.reader, self.writer)
        }

        /// Returns a reference to the inner reader.
        pub fn reader(&self) -> &R {
            &self.reader
        }

        /// Returns a reference to the inner writer.
        pub fn writer(&self) -> &W {
            &self.writer
        }

        /// Returns a mutable reference to the inner reader.
        pub fn reader_mut(&mut self) -> &mut R {
            &mut self.reader
        }

        /// Returns a mutable reference to the inner writer.
        pub fn writer_mut(&mut self) -> &mut W {
            &mut self.writer
        }
    }

    impl<R, W> AsyncRead for Join<R, W>
    where
        R: AsyncRead + Unpin,
    {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<Result<(), io::Error>> {
            self.project().reader.poll_read(cx, buf)
        }
    }

    impl<R, W> AsyncWrite for Join<R, W>
    where
        W: AsyncWrite + Unpin,
    {
        fn poll_write(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<Result<usize, io::Error>> {
            self.project().writer.poll_write(cx, buf)
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            self.project().writer.poll_flush(cx)
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Result<(), io::Error>> {
            self.project().writer.poll_shutdown(cx)
        }

        fn poll_write_vectored(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            bufs: &[io::IoSlice<'_>]
        ) -> Poll<Result<usize, io::Error>> {
            self.project().writer.poll_write_vectored(cx, bufs)
        }

        fn is_write_vectored(&self) -> bool {
            self.writer.is_write_vectored()
        }
    }

    impl<R, W> fmt::Debug for Join<R, W>
        where R: fmt::Debug, W: fmt::Debug
    {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt.debug_struct("Join")
                .field("writer", &self.writer)
                .field("reader", &self.reader)
                .finish()
        }
    }
}
