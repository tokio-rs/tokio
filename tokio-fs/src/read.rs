use crate::{file, File};
use futures::{try_ready, Async, Future, Poll};
use std::{io, mem, path::Path};
use tokio_io;

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
    P: AsRef<Path> + Send + 'static,
{
    ReadFile {
        state: State::Open(File::open(path)),
    }
}

/// A future used to open a file and read its entire contents into a buffer.
#[derive(Debug)]
pub struct ReadFile<P: AsRef<Path> + Send + 'static> {
    state: State<P>,
}

#[derive(Debug)]
enum State<P: AsRef<Path> + Send + 'static> {
    Open(file::OpenFuture<P>),
    Metadata(file::MetadataFuture),
    Read(tokio_io::io::ReadToEnd<File>),
}

impl<P: AsRef<Path> + Send + 'static> Future for ReadFile<P> {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let new_state = match &mut self.state {
            State::Open(ref mut open_file) => {
                let file = try_ready!(open_file.poll());
                State::Metadata(file.metadata())
            }
            State::Metadata(read_metadata) => {
                let (file, metadata) = try_ready!(read_metadata.poll());
                let buf = Vec::with_capacity(metadata.len() as usize + 1);
                let read = tokio_io::io::read_to_end(file, buf);
                State::Read(read)
            }
            State::Read(ref mut read) => {
                let (_, buf) = try_ready!(read.poll());
                return Ok(Async::Ready(buf));
            }
        };

        mem::replace(&mut self.state, new_state);
        // Getting here means we transitionsed state. Must poll the new state.
        self.poll()
    }
}
