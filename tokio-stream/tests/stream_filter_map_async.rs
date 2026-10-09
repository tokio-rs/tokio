use futures::Stream;
use tokio::sync::Notify;
use tokio_stream::{self as stream, StreamExt};
use tokio_test::{assert_pending, assert_ready_eq, task};

mod support {
    pub(crate) mod mpsc;
}

use support::mpsc;

#[tokio::test]
async fn basic() {
    let (tx, rx) = mpsc::unbounded_channel_stream();

    let mut st =
        task::spawn(rx.filter_map_async(async |x| if x % 2 == 0 { Some(x + 1) } else { None }));
    assert_pending!(st.poll_next());

    tx.send(1).unwrap();
    assert!(st.is_woken());
    assert_pending!(st.poll_next());

    tx.send(2).unwrap();
    assert!(st.is_woken());
    assert_ready_eq!(st.poll_next(), Some(3));

    assert_pending!(st.poll_next());

    tx.send(3).unwrap();
    assert!(st.is_woken());
    assert_pending!(st.poll_next());

    drop(tx);
    assert!(st.is_woken());
    assert_ready_eq!(st.poll_next(), None);
}

#[tokio::test]
async fn notify_unbounded() {
    let (tx, rx) = mpsc::unbounded_channel_stream();
    let notify = Notify::new();

    let mut st = task::spawn(rx.filter_map_async(async |x| {
        notify.notified().await;
        if x % 2 == 0 {
            Some(x + 1)
        } else {
            None
        }
    }));
    assert_pending!(st.poll_next());

    tx.send(0).unwrap();
    assert!(st.is_woken());
    assert_pending!(st.poll_next());

    notify.notify_one();
    assert!(st.is_woken());
    assert_ready_eq!(st.poll_next(), Some(1));

    tx.send(1).unwrap();
    assert!(!st.is_woken());
    assert_pending!(st.poll_next());

    notify.notify_one();
    assert!(st.is_woken());
    assert_pending!(st.poll_next());

    tx.send(2).unwrap();
    assert!(st.is_woken());
    assert_pending!(st.poll_next());

    notify.notify_one();
    assert!(st.is_woken());
    assert_ready_eq!(st.poll_next(), Some(3));

    drop(tx);
    assert!(!st.is_woken());
    assert_ready_eq!(st.poll_next(), None);
}

#[tokio::test]
async fn notify_bounded() {
    let notify = Notify::new();
    let mut st = task::spawn(stream::iter(0..3).filter_map_async(async |x| {
        notify.notified().await;
        if x % 2 == 0 {
            Some(x + 1)
        } else {
            None
        }
    }));
    assert_eq!(st.size_hint(), (0, Some(3)));
    assert_pending!(st.poll_next());

    notify.notify_one();
    assert!(st.is_woken());
    assert_eq!(st.size_hint(), (0, Some(3)));
    assert_ready_eq!(st.poll_next(), Some(1));
    assert_eq!(st.size_hint(), (0, Some(2)));

    notify.notify_one();
    assert!(!st.is_woken());
    assert_eq!(st.size_hint(), (0, Some(2)));
    assert_pending!(st.poll_next());
    assert_eq!(st.size_hint(), (0, Some(1)));

    notify.notify_one();
    assert!(st.is_woken());
    assert_eq!(st.size_hint(), (0, Some(1)));
    assert_ready_eq!(st.poll_next(), Some(3));
    assert_eq!(st.size_hint(), (0, Some(0)));

    assert!(!st.is_woken());
    assert_ready_eq!(st.poll_next(), None);
}
