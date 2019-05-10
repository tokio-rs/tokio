use crate::{file, File};
use futures::{try_ready, Async, Future, Poll};
use std::{fmt, io, mem, path::Path};
use tokio_io;

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
pub fn write<P, C: AsRef<[u8]>>(path: P, contents: C) -> WriteFile<P, C>
where
    P: AsRef<Path> + Send + 'static,
{
    WriteFile {
        state: State::Create(File::create(path), Some(contents)),
    }
}

/// A future used to open a file for writing and write the entire contents
/// of some data to it.
#[derive(Debug)]
pub struct WriteFile<P: AsRef<Path> + Send + 'static, C: AsRef<[u8]>> {
    state: State<P, C>,
}

#[derive(Debug)]
enum State<P: AsRef<Path> + Send + 'static, C: AsRef<[u8]>> {
    Create(file::CreateFuture<P>, Option<C>),
    Write(tokio_io::io::WriteAll<File, C>),
}

impl<P: AsRef<Path> + Send + 'static, C: AsRef<[u8]> + fmt::Debug> Future for WriteFile<P, C> {
    type Item = C;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let new_state = match &mut self.state {
            State::Create(ref mut create_file, contents) => {
                let file = try_ready!(create_file.poll());
                let write = tokio_io::io::write_all(file, contents.take().unwrap());
                State::Write(write)
            }
            State::Write(ref mut write) => {
                let (_, contents) = try_ready!(write.poll());
                return Ok(Async::Ready(contents));
            }
        };

        mem::replace(&mut self.state, new_state);
        // We just entered the Write state, need to poll it before returning.
        self.poll()
    }
}
