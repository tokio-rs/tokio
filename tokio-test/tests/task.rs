use std::future::poll_fn;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;
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
#[cfg_attr(miri, ignore)]
fn test_spawn_block_on() {
    let job = thread::spawn(move || {
        task::spawn(async {
            let mut poll_once = false;
            poll_fn(|cx| {
                if poll_once {
                    return Poll::Ready(());
                }
                assert!(!poll_once);
                poll_once = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            })
            .await;

            let mut once = false;
            poll_fn(|cx| {
                if once {
                    return Poll::Ready(());
                }
                let waker = cx.waker().clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(333));
                    waker.wake();
                });
                assert!(!once);
                once = true;
                Poll::Pending
            })
            .await;
        })
        .block_on();
    });

    let job2 = thread::spawn(|| {
        task::spawn(async { std::future::pending::<()>().await }).block_on();
    });

    thread::sleep(Duration::from_secs(2));
    assert!(job.is_finished());
    assert!(!job2.is_finished());
}
