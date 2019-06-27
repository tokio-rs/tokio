#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

/// Unwrap a ready value or propagate `Async::Pending`.
#[macro_export]
macro_rules! ready {
    ($e:expr) => {{
        use std::task::Poll::{Pending, Ready};

        match $e {
            Ready(v) => v,
            Pending => return Pending,
        }
    }};
}

#[path = "../src/oneshot.rs"]
#[allow(warnings)]
mod oneshot;

// use futures::{self, Async, Future};
use loom;
use loom::futures::{block_on, poll_future};
use loom::thread;

use std::task::Poll::{Pending, Ready};

#[test]
fn smoke() {
    loom::fuzz(|| {
        let (tx, rx) = oneshot::channel();

        thread::spawn(move || {
            tx.send(1).unwrap();
        });

        let value = block_on(rx).unwrap();
        assert_eq!(1, value);
    });
}

#[test]
fn changing_rx_task() {
    loom::fuzz(|| {
        let (tx, mut rx) = oneshot::channel();

        thread::spawn(move || {
            tx.send(1).unwrap();
        });

        let rx = thread::spawn(move || {
            match poll_future(&mut rx) {
                Ready(Ok(value)) => {
                    // ok
                    assert_eq!(1, value);
                    None
                }
                Ready(Err(_)) => unimplemented!(),
                Pending => Some(rx),
            }
        })
        .join()
        .unwrap();

        if let Some(rx) = rx {
            // Previous task parked, use a new task...
            let value = block_on(rx).unwrap();
            assert_eq!(1, value);
        }
    });
}

// TODO: Move this into `oneshot` proper.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

struct OnClose<'a> {
    tx: &'a mut oneshot::Sender<i32>,
}

impl<'a> OnClose<'a> {
    fn new(tx: &'a mut oneshot::Sender<i32>) -> Self {
        OnClose { tx }
    }
}

impl<'a> Future for OnClose<'a> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.get_mut().tx.poll_close(cx)
    }
}

#[test]
fn changing_tx_task() {
    loom::fuzz(|| {
        let (mut tx, rx) = oneshot::channel::<i32>();

        thread::spawn(move || {
            drop(rx);
        });

        let tx = thread::spawn(move || {
            let t1 = poll_future(&mut OnClose::new(&mut tx));

            match t1 {
                Ready(()) => None,
                Pending => Some(tx),
            }
        })
        .join()
        .unwrap();

        if let Some(mut tx) = tx {
            // Previous task parked, use a new task...
            block_on(OnClose::new(&mut tx));
        }
    });
}
