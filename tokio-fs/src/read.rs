use crate::{file, File};
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::{io, mem, path::Path};
use tokio_io;
use tokio_io::AsyncRead;

/// Creates a future which will open a file for reading and read the entire
/// contents into a buffer and return said buffer.
///
/// This is the async equivalent of `std::fs::read`.
///
/// # Examples
///
/// ```no_run
/// use tokio::prelude::Future;
///
/// let task = tokio::fs::read("foo.txt").map(|data| {
///     // do something with the contents of the file ...
///     println!("foo.txt contains {} bytes", data.len());
/// }).map_err(|e| {
///     // handle errors
///     eprintln!("IO error: {:?}", e);
/// });
///
/// tokio::run(task);
/// ```
pub fn read<P>(path: P) -> ReadFile<P>
where
    P: AsRef<Path> + Send + Unpin + 'static,
{
    ReadFile {
        state: State::Open(File::open(path)),
    }
}

/// A future used to open a file and read its entire contents into a buffer.
#[derive(Debug)]
pub struct ReadFile<P: AsRef<Path> + Send + Unpin + 'static> {
    state: State<P>,
}

#[derive(Debug)]
enum State<P: AsRef<Path> + Send + Unpin + 'static> {
    Open(file::OpenFuture<P>),
    Metadata(file::MetadataFuture),
    Read(Vec<u8>, File),
}

impl<P: AsRef<Path> + Send + Unpin + 'static> Future for ReadFile<P> {
    type Output = io::Result<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = Pin::get_mut(self);
        let new_state = match &mut inner.state {
            State::Open(ref mut open_file) => {
                let file = ready!(Pin::new(open_file).poll(cx))?;
                State::Metadata(file.metadata())
            }
            State::Metadata(read_metadata) => {
                let (file, metadata) = ready!(Pin::new(read_metadata).poll(cx))?;
                let buf = Vec::with_capacity(metadata.len() as usize + 1);
                State::Read(buf, file)
            }
            State::Read(buf, file) => {
                ready!(Pin::new(file).poll_read(cx, buf))?;
                return Poll::Ready(Ok(buf.to_vec()));
            }
        };

        mem::replace(&mut inner.state, new_state);
        // Getting here means we transitionsed state. Must poll the new state.
        Pin::new(inner).poll(cx)
    }
}
