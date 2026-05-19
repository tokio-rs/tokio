use crate::Stream;
use futures_core::stream::FusedStream;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::time::{Instant, Interval};

/// A wrapper around [`Interval`] that implements [`Stream`].
///
/// # Example
///
/// ```
/// use tokio::time::{Duration, Instant, interval};
/// use tokio_stream::wrappers::IntervalStream;
/// use tokio_stream::StreamExt;
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// let start = Instant::now();
/// let interval = interval(Duration::from_millis(10));
/// let mut stream = IntervalStream::new(interval);
/// for _ in 0..3 {
///     if let Some(instant) = stream.next().await {
///         println!("elapsed: {:.1?}", instant.duration_since(start));
///     }
/// }
/// # }
/// ```
///
/// [`Interval`]: struct@tokio::time::Interval
/// [`Stream`]: trait@crate::Stream
#[derive(Debug)]
#[cfg_attr(docsrs, doc(cfg(feature = "time")))]
pub struct IntervalStream {
    inner: Interval,
}

impl IntervalStream {
    /// Create a new `IntervalStream`.
    pub fn new(interval: Interval) -> Self {
        Self { inner: interval }
    }

    /// Get back the inner `Interval`.
    pub fn into_inner(self) -> Interval {
        self.inner
    }
}

impl Stream for IntervalStream {
    type Item = Instant;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Instant>> {
        self.inner.poll_tick(cx).map(Some)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}

impl FusedStream for IntervalStream {
    fn is_terminated(&self) -> bool {
        false
    }
}

impl AsRef<Interval> for IntervalStream {
    fn as_ref(&self) -> &Interval {
        &self.inner
    }
}

impl AsMut<Interval> for IntervalStream {
    fn as_mut(&mut self) -> &mut Interval {
        &mut self.inner
    }
}
