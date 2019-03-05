use super::File;
use blocking_pool::{spawn_blocking, BlockingFuture};

use futures::{Async, Future, Poll};

use std::fs::OpenOptions as StdOpenOptions;
use std::io;
use std::path::Path;

/// Future returned by `File::open` and resolves to a `File` instance.
#[derive(Debug)]
pub struct OpenFuture<P> {
    state: Option<State<P>>,
}

#[derive(Debug)]
struct Inner<P> {
    options: StdOpenOptions,
    path: P,
}

#[derive(Debug)]
enum State<P> {
    Idle(Inner<P>),
    Blocked(BlockingFuture<Result<std::fs::File, io::Error>>),
}

impl<P> OpenFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    pub(crate) fn new(options: StdOpenOptions, path: P) -> Self {
        OpenFuture {
            state: Some(State::Idle(Inner { options, path })),
        }
    }
}

impl<P> Future for OpenFuture<P>
where
    P: AsRef<Path> + Send + 'static,
{
    type Item = File;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.state.take().unwrap() {
            State::Idle(inner) => {
                if tokio_threadpool::entered() {
                    let std = try_ready!(::blocking_io(|| inner.options.open(&inner.path)));
                    return Ok(File::from_std(std).into());
                }

                self.state = Some(State::Blocked(spawn_blocking(move || {
                    inner.options.open(inner.path)
                })));
            }
            State::Blocked(job) => self.state = Some(State::Blocked(job)),
        }

        match self.state.as_mut().unwrap() {
            State::Idle(_) => unreachable!(),
            State::Blocked(job) => match job.poll().unwrap() {
                Async::NotReady => Ok(Async::NotReady),
                Async::Ready(Ok(std)) => Ok(Async::Ready(File::from_std(std))),
                Async::Ready(Err(err)) => Err(err),
            },
        }
    }
}
