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
    Write(Option<C>, File),
}

impl<P: AsRef<Path> + Send + Unpin + 'static, C: AsRef<[u8]> + Unpin + fmt::Debug> Future
    for WriteFile<P, C>
{
    type Output = io::Result<C>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = Pin::get_mut(self);
        let new_state = match &mut inner.state {
            State::Create(create_file, contents) => {
                let file = ready!(Pin::new(create_file).poll(cx))?;
                State::Write(contents.take(), file)
            }
            State::Write(buf, file) => {
                let buf = buf.take().unwrap();
                ready!(Pin::new(file).poll_write(cx, buf.as_ref()))?;
                return Poll::Ready(Ok(buf.into()));
            }
        };

        mem::replace(&mut inner.state, new_state);
        // We just entered the Write state, need to poll it before returning.
        Pin::new(inner).poll(cx)
    }
}
