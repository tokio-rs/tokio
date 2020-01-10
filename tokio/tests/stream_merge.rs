use tokio::stream::{self, Stream, StreamExt};
use tokio::sync::mpsc;
use tokio_test::task;
use tokio_test::{assert_pending, assert_ready};

#[tokio::test]
async fn merge_sync_streams() {
    let mut s = stream::iter(vec![0, 2, 4, 6]).merge(stream::iter(vec![1, 3, 5]));

    for i in 0..7 {
        let rem = 7 - i;
        assert_eq!(s.size_hint(), (rem, Some(rem)));
        assert_eq!(Some(i), s.next().await);
    }

    assert!(s.next().await.is_none());
}

#[tokio::test]
async fn merge_async_streams() {
    let (tx1, rx1) = mpsc::unbounded_channel();
    let (tx2, rx2) = mpsc::unbounded_channel();

    let mut rx = task::spawn(rx1.merge(rx2));

    assert_eq!(rx.size_hint(), (0, None));

    assert_pending!(rx.poll_next());

    tx1.send(1).unwrap();

    assert!(rx.is_woken());
    assert_eq!(Some(1), assert_ready!(rx.poll_next()));

    assert_pending!(rx.poll_next());
    tx2.send(2).unwrap();

    assert!(rx.is_woken());
    assert_eq!(Some(2), assert_ready!(rx.poll_next()));
    assert_pending!(rx.poll_next());

    drop(tx1);
    assert!(rx.is_woken());
    assert_pending!(rx.poll_next());

    tx2.send(3).unwrap();
    assert!(rx.is_woken());
    assert_eq!(Some(3), assert_ready!(rx.poll_next()));
    assert_pending!(rx.poll_next());

    drop(tx2);
    assert!(rx.is_woken());
    assert_eq!(None, assert_ready!(rx.poll_next()));
}
