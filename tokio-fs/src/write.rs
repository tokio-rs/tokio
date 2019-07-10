use crate::{file, File};
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::{fmt, io, mem, path::Path};
use tokio_io;
use tokio_io::AsyncWrite;

/// Creates a future that will open a file for writing and write the entire
/// contents of `contents` to it.
///
/// This is the async equivalent of `std::fs::write`.
///
/// # Examples
///
/// ```no_run
/// use tokio::prelude::Future;
///
/// let buffer = b"Hello world!";
/// let task = tokio::fs::write("foo.txt", buffer).map(|data| {
///     // `data` has now been written to foo.txt. The buffer is being
///     // returned so it can be used for other things.
///     println!("foo.txt now had {} bytes written to it", data.len());
/// }).map_err(|e| {
///     // handle errors
///     eprintln!("IO error: {:?}", e);
/// });
///
/// tokio::run(task);
/// ```
pub fn write<P, C: AsRef<[u8]> + Unpin>(path: P, contents: C) -> WriteFile<P, C>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    WriteFile {
        state: State::Create(File::create(path), Some(contents)),
    }
}

/// A future used to open a file for writing and write the entire contents
/// of some data to it.
#[derive(Debug)]
pub struct WriteFile<P: AsRef<Path> + Send + Unpin + 'static, C: AsRef<[u8]> + Unpin> {
    state: State<P, C>,
}

#[derive(Debug)]
enum State<P: AsRef<Path> + Send + Unpin + 'static, C: AsRef<[u8]> + Unpin> {
    Create(file::CreateFuture<P>, Option<C>),
    Writing { f: File, buf: C, pos: usize },
    Empty,
}

fn zero_write() -> io::Error {
    io::Error::new(io::ErrorKind::WriteZero, "zero-length write")
}

impl<P: AsRef<Path> + Send + Unpin + 'static, C: AsRef<[u8]> + Unpin + fmt::Debug> Future
    for WriteFile<P, C>
{
    type Output = io::Result<C>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = Pin::get_mut(self);
        match &mut inner.state {
            State::Create(create_file, contents) => {
                let file = ready!(Pin::new(create_file).poll(cx))?;
                let contents = contents.take().unwrap();
                let new_state = State::Writing {
                    f: file,
                    buf: contents,
                    pos: 0,
                };
                mem::replace(&mut inner.state, new_state);
                // We just entered the Write state, need to poll it before returning.
                return Pin::new(inner).poll(cx);
            }
            State::Empty => panic!("poll a WriteFile after it's done"),
            _ => {}
        }

        match mem::replace(&mut inner.state, State::Empty) {
            State::Writing {
                mut f,
                buf,
                mut pos,
            } => {
                let buf_ref = buf.as_ref();
                while pos < buf_ref.len() {
                    let n = ready!(Pin::new(&mut f).poll_write(cx, &buf_ref[pos..]))?;
                    pos += n;
                    if n == 0 {
                        return Poll::Ready(Err(zero_write()));
                    }
                }
                Poll::Ready(Ok(buf))
            }
            _ => panic!(),
        }
    }
}
