use std::future::{pending, Future};
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

#[test]
fn poll_to_block_ready() {
    let mut task = task::spawn(async { 42 });
    assert_eq!(task.poll_to_block(), Poll::Ready(42));
}

#[test]
fn poll_to_block_pending_not_woken() {
    let mut task = task::spawn(pending::<()>());
    assert!(task.poll_to_block().is_pending());
}

struct WakeThenReady {
    step: u8,
}

impl Future for WakeThenReady {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        match self.step {
            0 => {
                self.step = 1;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
            _ => Poll::Ready(()),
        }
    }
}

#[test]
fn poll_to_block_advances_on_wake() {
    let mut task = task::spawn(WakeThenReady { step: 0 });
    assert!(task.poll_to_block().is_ready());
}

struct WakeNTimes {
    remaining: u8,
}

impl Future for WakeNTimes {
    type Output = u8;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<u8> {
        if self.remaining == 0 {
            return Poll::Ready(0);
        }
        self.remaining -= 1;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[test]
fn poll_to_block_multiple_wakes() {
    let mut task = task::spawn(WakeNTimes { remaining: 3 });
    assert_eq!(task.poll_to_block(), Poll::Ready(0));
}

struct WakeForever;

impl Future for WakeForever {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

#[test]
#[should_panic(expected = "poll_to_block exceeded 150 iterations")]
fn poll_to_block_panics_on_infinite_wake() {
    let mut task = task::spawn(WakeForever);
    let _ = task.poll_to_block();
}
