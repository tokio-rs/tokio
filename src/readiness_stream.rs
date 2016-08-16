use std::io;

use futures::stream::Stream;
use futures::{Future, Task, Poll};
use futures_io::{Ready};

use event_loop::{IoSource, LoopHandle, AddSource};

/// A concrete implementation of a stream of readiness notifications for I/O
/// objects that originates from an event loop.
///
/// Created by the `ReadinessStream::new` method, each `ReadinessStream` is
/// associated with a specific event loop and source of events that will be
/// registered with an event loop.
///
/// Currently readiness streams have "edge" semantics. That is, if a stream
/// receives a readable notification it will not receive another readable
/// notification until all bytes have been read from the stream.
///
/// Note that the precise semantics of when notifications are received will
/// likely be configurable in the future.
pub struct ReadinessStream {
    io_token: usize,
    loop_handle: LoopHandle,
    source: IoSource,
}

pub struct ReadinessStreamNew {
    inner: AddSource,
    handle: Option<LoopHandle>,
    source: Option<IoSource>,
}

impl ReadinessStream {
    /// Creates a new readiness stream associated with the provided
    /// `loop_handle` and for the given `source`.
    ///
    /// This method returns a future which will resolve to the readiness stream
    /// when it's ready.
    pub fn new(loop_handle: LoopHandle, source: IoSource)
               -> ReadinessStreamNew {
        ReadinessStreamNew {
            inner: loop_handle.add_source(source.clone()),
            source: Some(source),
            handle: Some(loop_handle),
        }
    }
}

impl Future for ReadinessStreamNew {
    type Item = ReadinessStream;
    type Error = io::Error;

    fn poll(&mut self, task: &mut Task) -> Poll<ReadinessStream, io::Error> {
        self.inner.poll(task).map(|token| {
            ReadinessStream {
                io_token: token,
                source: self.source.take().unwrap(),
                loop_handle: self.handle.take().unwrap(),
            }
        })
    }
}

impl Stream for ReadinessStream {
    type Item = Ready;
    type Error = io::Error;

    fn poll(&mut self, task: &mut Task) -> Poll<Option<Ready>, io::Error> {
        match self.source.take_readiness() {
            None => {
                self.loop_handle.schedule(self.io_token, task);
                Poll::NotReady
            }
            Some(r) => {
                if !r.is_read() || !r.is_write() {
                    self.loop_handle.schedule(self.io_token, task);
                }
                Poll::Ok(Some(r))
            }
        }
    }
}

impl Drop for ReadinessStream {
    fn drop(&mut self) {
        self.loop_handle.drop_source(self.io_token)
    }
}
