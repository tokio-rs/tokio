use std::io;

use futures::stream::Stream;
use futures::{Future, Task, Poll};
use futures_io::{Ready, IoFuture};

use event_loop::{IoSource, LoopHandle};

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

impl ReadinessStream {
    /// Creates a new readiness stream associated with the provided
    /// `loop_handle` and for the given `source`.
    ///
    /// This method returns a future which will resolve to the readiness stream
    /// when it's ready.
    pub fn new(loop_handle: LoopHandle, source: IoSource)
               -> Box<IoFuture<ReadinessStream>> {
        loop_handle.add_source(source.clone()).map(|token| {
            ReadinessStream {
                io_token: token,
                source: source,
                loop_handle: loop_handle,
            }
        }).boxed()
    }
}

impl Stream for ReadinessStream {
    type Item = Ready;
    type Error = io::Error;

    fn poll(&mut self, _task: &mut Task) -> Poll<Option<Ready>, io::Error> {
        match self.source.take_readiness() {
            None => Poll::NotReady,
            Some(r) => Poll::Ok(Some(r)),
        }
    }

    fn schedule(&mut self, task: &mut Task) {
        self.loop_handle.schedule(self.io_token, task)
    }
}

impl Drop for ReadinessStream {
    fn drop(&mut self) {
        self.loop_handle.drop_source(self.io_token)
    }
}
