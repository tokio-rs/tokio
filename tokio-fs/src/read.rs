use crate::{file, File};

use tokio_io::AsyncRead;

use futures_core::ready;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::{io, mem, path::Path};

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
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct ReadFile<P: AsRef<Path> + Send + Unpin + 'static> {
    state: State<P>,
}

#[derive(Debug)]
enum State<P: AsRef<Path> + Send + Unpin + 'static> {
    Open(file::OpenFuture<P>),
    Metadata(file::MetadataFuture),
    Reading(Vec<u8>, usize, File),
    Empty,
}

impl<P: AsRef<Path> + Send + Unpin + 'static> Future for ReadFile<P> {
    type Output = io::Result<Vec<u8>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = Pin::get_mut(self);
        match &mut inner.state {
            State::Open(ref mut open_file) => {
                let file = ready!(Pin::new(open_file).poll(cx))?;
                let new_state = State::Metadata(file.metadata());
                mem::replace(&mut inner.state, new_state);
                Pin::new(inner).poll(cx)
            }
            State::Metadata(read_metadata) => {
                let (file, metadata) = ready!(Pin::new(read_metadata).poll(cx))?;
                let buf = Vec::with_capacity(metadata.len() as usize + 1);
                let new_state = State::Reading(buf, 0, file);
                mem::replace(&mut inner.state, new_state);
                Pin::new(inner).poll(cx)
            }
            State::Reading(buf, ref mut pos, file) => {
                let n = ready!(Pin::new(file).poll_read_buf(cx, buf))?;
                *pos += n;
                if *pos >= buf.len() {
                    match mem::replace(&mut inner.state, State::Empty) {
                        State::Reading(buf, _, _) => Poll::Ready(Ok(buf)),
                        _ => panic!(),
                    }
                } else {
                    Poll::Pending
                }
            }
            State::Empty => panic!("poll a WriteFile after it's done"),
        }
    }
}
