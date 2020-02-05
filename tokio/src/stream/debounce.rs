use crate::stream::Stream;
use crate::time::driver::Registration;
use crate::time::{Duration, Instant};

use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

pin_project! {
    /// Stream returned by the [`debounce`](super::StreamExt::debounce) method.
    pub struct Debounce<St>
    where
        St: Stream,
    {
        #[pin]
        stream: Option<St>,
        duration: Duration,
        next_item: Option<St::Item>,
        registration: Option<Registration>,
    }
}

impl<St> Debounce<St>
where
    St: Stream,
{
    pub(super) fn new(stream: St, duration: Duration) -> Debounce<St> {
        Debounce {
            stream: Some(stream),
            duration,
            next_item: None,
            registration: None,
        }
    }
}

impl<St> Stream for Debounce<St>
where
    St: Stream,
    St::Item: std::fmt::Debug,
{
    type Item = St::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<St::Item>> {
        let mut this = self.project();
        if let Some(mut stream) = Option::as_pin_mut(this.stream.as_mut()) {
            while let Poll::Ready(item) = stream.as_mut().poll_next(cx) {
                if item.is_some() {
                    *this.next_item = item;
                    *this.registration = None;
                } else {
                    this.stream.set(None);
                    break;
                }
            }
        }

        if let Some(registration) = this.registration {
            // `poll_elapsed` can return an error in two cases:
            //
            // - AtCapacity: this is a pathlogical case where far too many
            //   delays have been scheduled.
            // - Shutdown: No timer has been setup, which is a mis-use error.
            //
            // Both cases are extremely rare, and pretty accurately fit into
            // "logic errors", so we just panic in this case. A user couldn't
            // really do much better if we passed the error onwards.
            match ready!(registration.poll_elapsed(cx)) {
                Ok(()) => {
                    *this.registration = None;
                    return Poll::Ready(this.next_item.take());
                }
                Err(e) => panic!("timer error: {}", e),
            }
        }

        if this.next_item.is_some() {
            *this.registration = Some(Registration::new(
                Instant::now() + *this.duration,
                Duration::from_millis(0),
            ));
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }

        match Option::as_pin_mut(this.stream) {
            Some(_) => Poll::Pending,
            None => Poll::Ready(None),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.stream {
            Some(stream) => {
                let (mut lower, mut upper) = stream.size_hint();
                match self.next_item {
                    Some(_) => {
                        lower = lower.saturating_add(1);
                        upper = upper.map(|x| x.saturating_add(1));
                        (lower, upper)
                    }
                    None => (lower, upper),
                }
            }
            None => match self.next_item {
                Some(_) => (0, Some(1)),
                None => (0, Some(0)),
            },
        }
    }
}
