#![warn(rust_2018_idioms)]
#![cfg(feature = "sync")]

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
use wasm_bindgen_test::wasm_bindgen_test as test;

use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::Arc;

use tokio::sync::watch;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok};

#[test]
fn upgrade_while_sender_is_alive() {
    let (tx, mut rx) = watch::channel("one");
    let tx_weak = tx.downgrade();

    let tx2 = tx_weak.upgrade().expect("sender is still alive");
    assert_eq!(2, tx.sender_count());

    tx2.send("two").unwrap();

    {
        let mut t = spawn(rx.changed());
        assert_ready_ok!(t.poll());
    }

    assert_eq!(*rx.borrow(), "two");
}

#[test]
fn upgrade_fails_once_every_sender_is_dropped() {
    let (tx, rx) = watch::channel("one");
    let tx_weak = tx.downgrade();

    drop(tx);

    assert_eq!(0, tx_weak.sender_count());
    assert!(tx_weak.upgrade().is_none());

    // A live receiver does not make the channel upgradable again.
    drop(rx);
    assert!(tx_weak.upgrade().is_none());
}

#[test]
fn weak_sender_does_not_keep_the_channel_open() {
    let (tx, mut rx) = watch::channel("one");
    let _tx_weak = tx.downgrade();

    {
        let mut t = spawn(rx.changed());
        assert_pending!(t.poll());

        drop(tx);

        // The weak sender is the only remaining sending half, so the channel is
        // closed even though it is still alive.
        assert!(t.is_woken());
        assert_ready_err!(t.poll());
    }

    assert!(rx.has_changed().is_err());
}

#[test]
fn upgraded_sender_keeps_the_channel_open() {
    let (tx, mut rx) = watch::channel("one");
    let tx_weak = tx.downgrade();
    let tx2 = tx_weak.upgrade().expect("sender is still alive");

    drop(tx);

    // `tx2` is a full sender, so the channel is still open.
    {
        let mut t = spawn(rx.changed());
        assert_pending!(t.poll());
        tx2.send("two").unwrap();
        assert_ready_ok!(t.poll());
    }

    drop(tx2);

    {
        let mut t = spawn(rx.changed());
        assert_ready_err!(t.poll());
    }
}

#[test]
fn upgrade_downgrade_round_trip() {
    let (tx, _rx) = watch::channel("one");

    let tx_weak = tx.downgrade();
    let tx2 = tx_weak.upgrade().unwrap();
    let tx_weak2 = tx2.downgrade();

    assert!(tx_weak.same_channel(&tx_weak2));
    assert_eq!(2, tx.sender_count());
    assert_eq!(2, tx.weak_sender_count());

    assert!(tx_weak2.upgrade().is_some());
}

#[test]
fn sender_and_weak_sender_counts() {
    let (tx, _rx) = watch::channel("one");
    assert_eq!(1, tx.sender_count());
    assert_eq!(0, tx.weak_sender_count());

    let tx_weak = tx.downgrade();
    assert_eq!(1, tx.sender_count());
    assert_eq!(1, tx.weak_sender_count());
    assert_eq!(1, tx_weak.sender_count());
    assert_eq!(1, tx_weak.weak_sender_count());

    let tx_weak2 = tx_weak.clone();
    assert_eq!(2, tx.weak_sender_count());

    let tx2 = tx.clone();
    assert_eq!(2, tx_weak.sender_count());

    drop(tx_weak2);
    assert_eq!(1, tx.weak_sender_count());

    drop(tx2);
    assert_eq!(1, tx_weak.sender_count());

    drop(tx);
    assert_eq!(0, tx_weak.sender_count());
    assert_eq!(1, tx_weak.weak_sender_count());
}

#[test]
fn dropping_a_weak_sender_leaves_the_channel_open() {
    let (tx, mut rx) = watch::channel("one");
    let tx_weak = tx.downgrade();
    let tx_weak2 = tx.downgrade();

    drop(tx_weak);
    drop(tx_weak2);

    // Only weak senders went away, so the channel is untouched.
    assert_eq!(1, tx.sender_count());
    assert_eq!(0, tx.weak_sender_count());
    assert!(rx.has_changed().is_ok());

    tx.send("two").unwrap();

    {
        let mut t = spawn(rx.changed());
        assert_ready_ok!(t.poll());
    }

    assert_eq!(*rx.borrow(), "two");
}

#[test]
fn weak_sender_same_channel() {
    let (tx, _rx) = watch::channel("one");
    let (tx2, _rx2) = watch::channel("one");

    let tx_weak = tx.downgrade();
    let tx_weak_clone = tx_weak.clone();
    let tx2_weak = tx2.downgrade();

    assert!(tx_weak.same_channel(&tx_weak_clone));
    assert!(!tx_weak.same_channel(&tx2_weak));
}

#[test]
fn weak_sender_keeps_the_value_alive() {
    struct CountOnDrop(Arc<AtomicUsize>);

    impl Drop for CountOnDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, SeqCst);
        }
    }

    let drops = Arc::new(AtomicUsize::new(0));
    let (tx, rx) = watch::channel(CountOnDrop(drops.clone()));
    let tx_weak = tx.downgrade();

    drop(tx);
    drop(rx);

    // The channel is closed, but the weak sender still holds the allocation, so
    // the value has not been dropped yet.
    assert_eq!(0, drops.load(SeqCst));

    drop(tx_weak);
    assert_eq!(1, drops.load(SeqCst));
}

#[test]
fn receiver_subscribed_from_upgraded_sender_sees_current_value() {
    let (tx, _rx) = watch::channel("one");
    let tx_weak = tx.downgrade();

    tx.send("two").unwrap();

    let tx2 = tx_weak.upgrade().unwrap();
    let mut rx2 = tx2.subscribe();

    assert_eq!(*rx2.borrow_and_update(), "two");

    let mut t = spawn(rx2.changed());
    assert_pending!(t.poll());
}

#[tokio::test]
async fn weak_sender_across_tasks() {
    let (tx, mut rx) = watch::channel(0);
    let tx_weak = tx.downgrade();

    let handle = tokio::spawn(async move {
        let tx = tx_weak.upgrade()?;
        tx.send(1).unwrap();
        Some(tx.downgrade())
    });

    let tx_weak = handle.await.unwrap().expect("upgrade while tx is alive");

    rx.changed().await.unwrap();
    assert_eq!(*rx.borrow_and_update(), 1);

    drop(tx);

    assert!(tx_weak.upgrade().is_none());
    assert!(rx.changed().await.is_err());
}
