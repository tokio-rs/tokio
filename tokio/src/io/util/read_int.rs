use crate::io::{AsyncRead, ReadBuf};

use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::io::ErrorKind::UnexpectedEof;
use std::marker::PhantomPinned;
use std::pin::Pin;
use std::task::{Context, Poll};

macro_rules! reader {
    ($name:ident, $ty:ty, $convert:ident) => {
        reader!($name, $ty, $convert, std::mem::size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $convert:ident, $bytes:expr) => {
        pin_project! {
            #[doc(hidden)]
            #[must_use = "futures do nothing unless you `.await` or poll them"]
            pub struct $name<R> {
                #[pin]
                src: R,
                buf: [u8; $bytes],
                read: u8,
                // Make this future `!Unpin` for compatibility with async trait methods.
                #[pin]
                _pin: PhantomPinned,
            }
        }

        impl<R> $name<R> {
            pub(crate) fn new(src: R) -> Self {
                $name {
                    src,
                    buf: [0; $bytes],
                    read: 0,
                    _pin: PhantomPinned,
                }
            }
        }

        impl<R> Future for $name<R>
        where
            R: AsyncRead,
        {
            type Output = io::Result<$ty>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut me = self.project();

                let mut buf = ReadBuf::new(&mut me.buf[*me.read as usize..]);

                let result = me.src.as_mut().poll_read_exact(cx, &mut buf);
                *me.read += buf.filled().len() as u8;
                std::task::ready!(result)?;

                Poll::Ready(Ok(<$ty>::$convert(*me.buf)))
            }
        }
    };
}

macro_rules! reader8 {
    ($name:ident, $ty:ty) => {
        pin_project! {
            /// Future returned from `read_u8`
            #[doc(hidden)]
            #[must_use = "futures do nothing unless you `.await` or poll them"]
            pub struct $name<R> {
                #[pin]
                reader: R,
                // Make this future `!Unpin` for compatibility with async trait methods.
                #[pin]
                _pin: PhantomPinned,
            }
        }

        impl<R> $name<R> {
            pub(crate) fn new(reader: R) -> $name<R> {
                $name {
                    reader,
                    _pin: PhantomPinned,
                }
            }
        }

        impl<R> Future for $name<R>
        where
            R: AsyncRead,
        {
            type Output = io::Result<$ty>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let me = self.project();

                let mut buf = [0; 1];
                let mut buf = ReadBuf::new(&mut buf);
                match me.reader.poll_read(cx, &mut buf) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                    Poll::Ready(Ok(())) => {
                        if buf.filled().len() == 0 {
                            return Poll::Ready(Err(UnexpectedEof.into()));
                        }

                        Poll::Ready(Ok(buf.filled()[0] as $ty))
                    }
                }
            }
        }
    };
}

reader8!(ReadU8, u8);
reader8!(ReadI8, i8);

reader!(ReadU16, u16, from_be_bytes);
reader!(ReadU32, u32, from_be_bytes);
reader!(ReadU64, u64, from_be_bytes);
reader!(ReadU128, u128, from_be_bytes);

reader!(ReadI16, i16, from_be_bytes);
reader!(ReadI32, i32, from_be_bytes);
reader!(ReadI64, i64, from_be_bytes);
reader!(ReadI128, i128, from_be_bytes);

reader!(ReadF32, f32, from_be_bytes);
reader!(ReadF64, f64, from_be_bytes);

reader!(ReadU16Le, u16, from_le_bytes);
reader!(ReadU32Le, u32, from_le_bytes);
reader!(ReadU64Le, u64, from_le_bytes);
reader!(ReadU128Le, u128, from_le_bytes);

reader!(ReadI16Le, i16, from_le_bytes);
reader!(ReadI32Le, i32, from_le_bytes);
reader!(ReadI64Le, i64, from_le_bytes);
reader!(ReadI128Le, i128, from_le_bytes);

reader!(ReadF32Le, f32, from_le_bytes);
reader!(ReadF64Le, f64, from_le_bytes);
