use crate::io::AsyncWrite;

use bytes::BufMut;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io;
use std::mem::size_of;
use std::pin::Pin;
use std::task::{Context, Poll};

macro_rules! writer {
    ($name:ident, $ty:ty, $writer:ident) => {
        writer!($name, $ty, $writer, size_of::<$ty>());
    };
    ($name:ident, $ty:ty, $writer:ident, $bytes:expr) => {
        pin_project! {
            #[doc(hidden)]
            pub struct $name<W> {
                #[pin]
                dst: W,
                buf: [u8; $bytes],
                written: u8,
            }
        }

        impl<W> $name<W> {
            pub(crate) fn new(w: W, value: $ty) -> Self {
                let mut writer = $name {
                    buf: [0; $bytes],
                    written: 0,
                    dst: w,
                };
                BufMut::$writer(&mut &mut writer.buf[..], value);
                writer
            }
        }

        impl<W> Future for $name<W>
        where
            W: AsyncWrite,
        {
            type Output = io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let mut me = self.project();

                if *me.written == $bytes as u8 {
                    return Poll::Ready(Ok(()));
                }

                while *me.written < $bytes as u8 {
                    *me.written += match me
                        .dst
                        .as_mut()
                        .poll_write(cx, &me.buf[*me.written as usize..])
                    {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(Err(e)) => return Poll::Ready(Err(e.into())),
                        Poll::Ready(Ok(0)) => {
                            return Poll::Ready(Err(io::ErrorKind::WriteZero.into()));
                        }
                        Poll::Ready(Ok(n)) => n as u8,
                    };
                }
                Poll::Ready(Ok(()))
            }
        }
    };
}

macro_rules! writer8 {
    ($name:ident, $ty:ty) => {
        pin_project! {
            #[doc(hidden)]
            pub struct $name<W> {
                #[pin]
                dst: W,
                byte: $ty,
            }
        }

        impl<W> $name<W> {
            pub(crate) fn new(dst: W, byte: $ty) -> Self {
                Self { dst, byte }
            }
        }

        impl<W> Future for $name<W>
        where
            W: AsyncWrite,
        {
            type Output = io::Result<()>;

            fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
                let me = self.project();

                let buf = [*me.byte as u8];

                match me.dst.poll_write(cx, &buf[..]) {
                    Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
                    Poll::Ready(Ok(0)) => Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                    Poll::Ready(Ok(1)) => Poll::Ready(Ok(())),
                    Poll::Ready(Ok(_)) => unreachable!(),
                }
            }
        }
    };
}

writer8!(WriteU8, u8);
writer8!(WriteI8, i8);

writer!(WriteU16, u16, put_u16);
writer!(WriteU32, u32, put_u32);
writer!(WriteU64, u64, put_u64);
writer!(WriteU128, u128, put_u128);

writer!(WriteI16, i16, put_i16);
writer!(WriteI32, i32, put_i32);
writer!(WriteI64, i64, put_i64);
writer!(WriteI128, i128, put_i128);
