use futures_sink::Sink;
use pin_project_lite::pin_project;
use std::collections::VecDeque;
use std::fmt::Display;
use std::io;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;

/// A helper structure to manage unfinished sink writes.
/// This structures carries elements which are yet to be sent and
/// has a counter for elements which have been sent but are not flushed yet.
#[derive(Debug)]
struct CachedState {
    unflushed_bytes: usize,
    unsent_items: VecDeque<u8>,
}

impl CachedState {
    /// Creates a new [`CachedState<T>`].
    fn new() -> Self {
        Self {
            unflushed_bytes: 0,
            unsent_items: VecDeque::new(),
        }
    }

    /// Returns the unflushed bytes of this [`CachedState`].
    fn get_unflushed_bytes(&self) -> usize {
        self.unflushed_bytes
    }

    /// Resets the unflushed bytes of this [`CachedState`].
    fn reset_unflushed_bytes(&mut self) {
        self.unflushed_bytes = 0
    }
    /// Adds number of unflushed bytes to cache.
    fn add_unflushed_bytes(&mut self, n: usize) {
        self.unflushed_bytes += n
    }

    /// Returns the next cached element of this [`CachedState`].
    fn next_cache_element(&mut self) -> Option<u8> {
        self.unsent_items.pop_front()
    }

    /// Adds slice to cache of this [`CachedState`].
    fn add_elements_to_cache(&mut self, buf: &[u8]) {
        self.unsent_items
            .append(&mut buf.iter().copied().collect::<VecDeque<_>>())
    }

    /// Checks whether this [`CachedState`] contains sendable elements.
    fn has_cached_elements(&self) -> bool {
        !self.unsent_items.is_empty()
    }

    /// Returns the cached items of this [`CachedState`].
    fn get_unsent_items(&self) -> Vec<u8> {
        let (b, f) = self.unsent_items.as_slices();
        [b, f].concat()
    }
}

pin_project! {
    /// Convert a [`Sink`] of byte chunks into an [`AsyncWrite`].
    /// Bytes are sent into the sink and the sink is flushed once all bytes are sent. If an error occurs during the sending progress,
    /// the number of sent but unflushed bytes are saved in case the flushing operation stays unsuccessful.
    /// For the inverse operation of defining an [`AsyncWrite`] from a [`Sink`] you need to define a [`codec`].
    ///
    /// # Example
    ///
    /// ```
    /// use futures_util::SinkExt;
    /// use std::io::{Error, ErrorKind};
    /// use tokio::io::AsyncWriteExt;
    /// use tokio_util::io::SinkWriter;
    /// use tokio_util::sync::PollSender;
    ///
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Error> {
    ///  // Construct a channel pair to send data across and wrap a pollable sink.
    ///  // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
    ///  // as its error type.
    ///  let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(10);
    ///  let mut writer = SinkWriter::new(
    ///      PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::Other)),
    ///  );
    ///  // Write data to our interface...
    ///  let data: [u8; 4] = [1, 2, 3, 4];
    ///  let _ = writer.write(&data).await?;
    ///  // ... and receive it.
    ///  let mut received = Vec::new();
    ///  for _ in 0..4 {
    ///      received.push(rx.recv().await.unwrap());
    ///  }
    /// assert_eq!(&data, received.as_slice());
    ///
    /// # Ok(())
    /// # }
    /// ```
    ///
    ///
    /// [`AsyncWrite`]: tokio::io::AsyncWrite
    /// [`Sink`]: futures_sink::Sink
    /// [`codec`]: tokio_util::codec
    #[derive(Debug)]
    pub struct SinkWriter<S, T>
    where
        S: Sink<T>,
        T: From<u8>
    {
        #[pin]
        inner: S,
        cache: CachedState,
        phantom: PhantomData<T>
    }
}
impl<S, T> SinkWriter<S, T>
where
    S: Sink<T>,
    T: From<u8>,
{
    /// Creates a new [`SinkWriter`].
    pub fn new(sink: S) -> Self {
        Self {
            inner: sink,
            cache: CachedState::new(),
            phantom: PhantomData,
        }
    }

    /// Gets a reference to the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn get_ref(&self) -> &S {
        &self.inner
    }

    /// Gets a mutable reference to the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn get_mut(&mut self) -> &mut S {
        &mut self.inner
    }

    /// Consumes this `SinkWriter`, returning the underlying sink.
    ///
    /// It is inadvisable to directly write to the underlying sink.
    pub fn into_inner(self) -> S {
        self.inner
    }

    /// Returns the remaining unsent buffer of this [`SinkWriter<S, T>`].
    pub fn remaining_unsent_buffer(&self) -> Vec<u8> {
        self.cache.get_unsent_items()
    }
}

impl<S, E, T> AsyncWrite for SinkWriter<S, T>
where
    S: Sink<T, Error = E>,
    E: Into<io::Error> + std::fmt::Debug,
    T: From<u8>,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.as_mut().project().inner.poll_ready(cx) {
            Poll::Ready(Ok(_)) => {
                // We start with trying to empty potentially unsent items.
                let cache_bytes_written = if self.as_mut().project().cache.has_cached_elements() {
                    let len_cache = self.as_mut().project().cache.unsent_items.len();
                    let mut last_from_cache = 0;
                    loop {
                        // In case the sink is ready to receive items, try to send the next item through.
                        // If not, add remaining items to the buffer and return immediately.
                        match self.as_mut().project().inner.poll_ready(cx) {
                            Poll::Ready(Ok(())) => {
                                if let Some(b) = self.as_mut().project().cache.next_cache_element()
                                {
                                    if self.as_mut().project().inner.start_send(b.into()).is_err() {
                                        break last_from_cache;
                                    }
                                } else {
                                    // After sending items from the cache, return the number of unflushed bytes that were sent.
                                    break len_cache;
                                }
                            }
                            Poll::Ready(Err(e)) => {
                                let cache = self.as_mut().project().cache;
                                cache.add_elements_to_cache(buf);
                                cache.add_unflushed_bytes(last_from_cache);
                                return Poll::Ready(Err(e.into()));
                            }
                            Poll::Pending => {
                                let cache = self.as_mut().project().cache;
                                cache.add_elements_to_cache(buf);
                                cache.add_unflushed_bytes(last_from_cache);
                                cx.waker().wake_by_ref();
                                return Poll::Pending;
                            }
                        };

                        last_from_cache += 1;
                    }
                } else {
                    0
                };

                let buf_bytes_written = {
                    let mut written_until_problem = None;
                    for (i, b) in buf.iter().enumerate() {
                        match self.as_mut().project().inner.poll_ready(cx) {
                            Poll::Ready(Ok(())) => {
                                if self
                                    .as_mut()
                                    .project()
                                    .inner
                                    .start_send(From::from(*b))
                                    .is_err()
                                {
                                    let _ = written_until_problem.insert(i);
                                    break;
                                };
                            }
                            Poll::Ready(Err(e)) => {
                                let cache = self.as_mut().project().cache;
                                cache.add_elements_to_cache(&buf[i..]);
                                cache
                                    .add_unflushed_bytes(written_until_problem.unwrap_or_default());
                                return Poll::Ready(Err(e.into()));
                            }
                            Poll::Pending => {
                                let cache = self.as_mut().project().cache;
                                cache.add_elements_to_cache(&buf[i..]);
                                cache
                                    .add_unflushed_bytes(written_until_problem.unwrap_or_default());
                                cx.waker().wake_by_ref();
                                return Poll::Pending;
                            }
                        }
                    }
                    written_until_problem.unwrap_or(buf.len())
                };

                let bytes_written = buf_bytes_written + cache_bytes_written;
                match self.as_mut().project().inner.poll_flush(cx) {
                    Poll::Ready(Ok(_)) => {
                        let c = self.as_mut().project().cache;
                        let summed_bytes = bytes_written + c.get_unflushed_bytes();
                        c.reset_unflushed_bytes();
                        Poll::Ready(Ok(summed_bytes))
                    }
                    Poll::Ready(Err(e)) => {
                        self.as_mut()
                            .project()
                            .cache
                            .add_unflushed_bytes(bytes_written);
                        Poll::Ready(Err(e.into()))
                    }
                    Poll::Pending => {
                        self.as_mut()
                            .project()
                            .cache
                            .add_unflushed_bytes(bytes_written);
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e.into())),
            Poll::Pending => {
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_flush(cx).map_err(Into::into)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        self.project().inner.poll_close(cx).map_err(Into::into)
    }
}
