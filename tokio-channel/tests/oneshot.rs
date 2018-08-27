extern crate tokio_channel;
extern crate futures;

mod support;
use support::*;

use tokio_channel::oneshot::*;

use futures::prelude::*;
use futures::future::{lazy, ok};

use std::sync::mpsc;
use std::thread;

#[test]
fn smoke_poll() {
    let (mut tx, rx) = channel::<u32>();

    lazy(|| {
        assert!(tx.poll_cancel().unwrap().is_not_ready());
        assert!(tx.poll_cancel().unwrap().is_not_ready());
        drop(rx);
        assert!(tx.poll_cancel().unwrap().is_ready());
        assert!(tx.poll_cancel().unwrap().is_ready());
        ok::<(), ()>(())
    }).wait().unwrap();
}

#[test]
fn cancel_notifies() {
    let (tx, rx) = channel::<u32>();
    let (tx2, rx2) = mpsc::channel();

    WaitForCancel { tx: tx }.then(move |v| tx2.send(v)).forget();
    drop(rx);
    rx2.recv().unwrap().unwrap();
}

struct WaitForCancel {
    tx: Sender<u32>,
}

impl Future for WaitForCancel {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        self.tx.poll_cancel()
    }
}

#[test]
fn cancel_lots() {
    let (tx, rx) = mpsc::channel::<(Sender<_>, mpsc::Sender<_>)>();
    let t = thread::spawn(move || {
        for (tx, tx2) in rx {
            WaitForCancel { tx: tx }.then(move |v| tx2.send(v)).forget();
        }

    });

    for _ in 0..20000 {
        let (otx, orx) = channel::<u32>();
        let (tx2, rx2) = mpsc::channel();
        tx.send((otx, tx2)).unwrap();
        drop(orx);
        rx2.recv().unwrap().unwrap();
    }
    drop(tx);

    t.join().unwrap();
}

#[test]
fn close() {
    let (mut tx, mut rx) = channel::<u32>();
    rx.close();
    assert!(rx.poll().is_err());
    assert!(tx.poll_cancel().unwrap().is_ready());
}

#[test]
fn close_wakes() {
    let (tx, mut rx) = channel::<u32>();
    let (tx2, rx2) = mpsc::channel();
    let t = thread::spawn(move || {
        rx.close();
        rx2.recv().unwrap();
    });
    WaitForCancel { tx: tx }.wait().unwrap();
    tx2.send(()).unwrap();
    t.join().unwrap();
}

#[test]
fn is_canceled() {
    let (tx, rx) = channel::<u32>();
    assert!(!tx.is_canceled());
    drop(rx);
    assert!(tx.is_canceled());
}

#[test]
fn cancel_sends() {
    let (tx, rx) = mpsc::channel::<Sender<_>>();
    let t = thread::spawn(move || {
        for otx in rx {
            let _ = otx.send(42);
        }
    });

    for _ in 0..20000 {
        let (otx, mut orx) = channel::<u32>();
        tx.send(otx).unwrap();

        orx.close();
        // Not necessary to wrap in a task because the implementation of oneshot
        // never calls `task::current()` if the channel has been closed already.
        let _ = orx.poll();
    }

    drop(tx);
    t.join().unwrap();
}
