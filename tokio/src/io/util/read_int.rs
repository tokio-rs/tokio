use crate::io::AsyncRead;

use bytes::Buf;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::io::ErrorKind::UnexpectedEof;
use std::mem::size_of;
use std::pin::Pin;
use std::task::{Context, Poll};

macro_rules! reader {
    ($name:ident, $ty:ty, $reader:ident) => {
        reader!($name, $ty, $reader, size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $reader:ident, $bytes:expr) => {
        pin_project! {
            #[doc(hidden)]
            pub struct $name<R> {
                #[pin]
                src: R,
                buf: [u8; $bytes],
                read: u8,
            }
        }

        impl<R> $name<R> {
            pub(crate) fn new(src: R) -> Self {
                $name {
                    src,
                    buf: [0; $bytes],
                    read: 0,
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

                if *me.read == $bytes as u8 {
                    return Poll::Ready(Ok(Buf::$reader(&mut &me.buf[..])));
                }

                while *me.read < $bytes as u8 {
                    *me.read += match me
                        .src
                        .as_mut()
                        .poll_read(cx, &mut me.buf[*me.read as usize..])
                    {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                        Poll::Ready(Ok(0)) => {
                            return Poll::Ready(Err(UnexpectedEof.into()));
                        }
                        Poll::Ready(Ok(n)) => n as u8,
                    };
                }

                let num = Buf::$reader(&mut &me.buf[..]);

                Poll::Ready(Ok(num))
            }
        }
    };
}

macro_rules! reader8 {
    ($name:ident, $ty:ty) => {
        pin_project! {
            /// Future returned from `read_u8`
            #[doc(hidden)]
            pub struct $name<R> {
                #[pin]
                reader: R,
            }
        }

        impl<R> $name<R> {
            pub(crate) fn new(reader: R) -> $name<R> {
                $name { reader }
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
                match me.reader.poll_read(cx, &mut buf[..]) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                    Poll::Ready(Ok(0)) => Poll::Ready(Err(UnexpectedEof.into())),
                    Poll::Ready(Ok(1)) => Poll::Ready(Ok(buf[0] as $ty)),
                    Poll::Ready(Ok(_)) => unreachable!(),
                }
            }
        }
    };
}

reader8!(ReadU8, u8);
reader8!(ReadI8, i8);

reader!(ReadU16, u16, get_u16);
reader!(ReadU32, u32, get_u32);
reader!(ReadU64, u64, get_u64);
reader!(ReadU128, u128, get_u128);

reader!(ReadI16, i16, get_i16);
reader!(ReadI32, i32, get_i32);
reader!(ReadI64, i64, get_i64);
reader!(ReadI128, i128, get_i128);
