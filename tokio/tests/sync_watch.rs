#![allow(clippy::cognitive_complexity)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::watch;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

#[test]
fn single_rx_recv() {
    let (tx, mut rx) = watch::channel("one");

    {
        let mut t = spawn(rx.recv());
        let v = assert_ready!(t.poll());
        assert_eq!(v, "one");
    }

    {
        let mut t = spawn(rx.recv());

        assert_pending!(t.poll());

        tx.send("two").unwrap();

        assert!(t.is_woken());

        let v = assert_ready!(t.poll());
        assert_eq!(v, "two");
    }

    {
        let mut t = spawn(rx.recv());

        assert_pending!(t.poll());

        drop(tx);

        let res = assert_ready!(t.poll());
        assert_eq!(res, "two");
    }
}

#[test]
fn multi_rx() {
    let (tx, mut rx1) = watch::channel("one");
    let mut rx2 = rx1.clone();

    {
        let mut t1 = spawn(rx1.recv());
        let mut t2 = spawn(rx2.recv());

        let res = assert_ready!(t1.poll());
        assert_eq!(res, "one");

        let res = assert_ready!(t2.poll());
        assert_eq!(res, "one");
    }

    let mut t2 = spawn(rx2.recv());

    {
        let mut t1 = spawn(rx1.recv());

        assert_pending!(t1.poll());
        assert_pending!(t2.poll());

        tx.send("two").unwrap();

        assert!(t1.is_woken());
        assert!(t2.is_woken());

        let res = assert_ready!(t1.poll());
        assert_eq!(res, "two");
    }

    {
        let mut t1 = spawn(rx1.recv());

        assert_pending!(t1.poll());

        tx.send("three").unwrap();

        assert!(t1.is_woken());
        assert!(t2.is_woken());

        let res = assert_ready!(t1.poll());
        assert_eq!(res, "three");

        let res = assert_ready!(t2.poll());
        assert_eq!(res, "three");
    }

    drop(t2);

    {
        let mut t1 = spawn(rx1.recv());
        let mut t2 = spawn(rx2.recv());

        assert_pending!(t1.poll());
        assert_pending!(t2.poll());

        tx.send("four").unwrap();

        let res = assert_ready!(t1.poll());
        assert_eq!(res, "four");
        drop(t1);

        let mut t1 = spawn(rx1.recv());
        assert_pending!(t1.poll());

        drop(tx);

        assert!(t1.is_woken());
        let res = assert_ready!(t1.poll());
        assert_eq!(res, "four");

        let res = assert_ready!(t2.poll());
        assert_eq!(res, "four");

        drop(t2);
        let mut t2 = spawn(rx2.recv());
        let res = assert_ready!(t2.poll());
        assert_eq!(res, "four");
    }
}

#[test]
fn rx_observes_final_value() {
    // Initial value

    let (tx, mut rx) = watch::channel("one");
    drop(tx);

    {
        let mut t1 = spawn(rx.recv());
        let res = assert_ready!(t1.poll());
        assert_eq!(res, "one");
    }

    {
        let mut t1 = spawn(rx.recv());
        let res = assert_ready!(t1.poll());
        assert_eq!(res, "one");
    }

    // Sending a value

    let (tx, mut rx) = watch::channel("one");

    tx.send("two").unwrap();

    {
        let mut t1 = spawn(rx.recv());
        let res = assert_ready!(t1.poll());
        assert_eq!(res, "two");
    }

    {
        let mut t1 = spawn(rx.recv());
        assert_pending!(t1.poll());

        tx.send("three").unwrap();
        drop(tx);

        assert!(t1.is_woken());

        let res = assert_ready!(t1.poll());
        assert_eq!(res, "three");
    }

    {
        let mut t1 = spawn(rx.recv());
        let res = assert_ready!(t1.poll());
        assert_eq!(res, "three");
    }
}

#[test]
fn poll_close() {
    let (mut tx, rx) = watch::channel("one");

    {
        let mut t = spawn(tx.closed());
        assert_pending!(t.poll());

        drop(rx);

        assert!(t.is_woken());
        assert_ready!(t.poll());
    }

    assert!(tx.send("two").is_err());
}

#[test]
fn stream_impl() {
    use tokio::stream::StreamExt;

    let (tx, mut rx) = watch::channel("one");

    {
        let mut t = spawn(rx.next());
        let v = assert_ready!(t.poll()).unwrap();
        assert_eq!(v, "one");
    }

    {
        let mut t = spawn(rx.next());

        assert_pending!(t.poll());

        tx.send("two").unwrap();

        assert!(t.is_woken());

        let v = assert_ready!(t.poll()).unwrap();
        assert_eq!(v, "two");
    }

    {
        let mut t = spawn(rx.next());

        assert_pending!(t.poll());

        drop(tx);

        let res = assert_ready!(t.poll());
        assert_eq!(res, Some("two"));
    }
}
