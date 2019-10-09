use tokio_io::{AsyncRead, AsyncWrite};

#[pin_project]
pub struct AsTokio<T> {
    #[pin]
    inner: T,
}

pub trait AsTokio {
    fn as_tokio(self) -> AsTokio<Self>
}

#[cfg(feature = "futures-io")]
mod futures_io {
    use super::*;

    impl<W> AsyncRead for AsTokio<W> where W: futures_io::AsyncRead {
        fn poll_read(
            self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut [u8],
        ) -> Poll<io::Result<usize>> {
            let AsTokio { inner } = self.project();
            futures_io::AsyncRead::poll_read(inner, cx, buf)
        }
    }

}
