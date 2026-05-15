use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_stream::Stream;
use tokio_test::task;

/// A [`Stream`] that has a stub size hint.
struct SizedStream;

impl Stream for SizedStream {
    type Item = ();

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Pending
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (100, Some(200))
    }
}

#[test]
fn test_spawn_stream_size_hint() {
    let spawn = task::spawn(SizedStream);
    assert_eq!(spawn.size_hint(), (100, Some(200)));
}
