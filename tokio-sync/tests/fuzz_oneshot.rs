extern crate futures;
extern crate loom;

#[path = "../src/oneshot.rs"]
#[allow(warnings)]
mod oneshot;

use futures::{Async, Future};
use loom::futures::block_on;
use loom::thread;

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
            let t1 = block_on(futures::future::poll_fn(|| Ok::<_, ()>(rx.poll().into()))).unwrap();

            match t1 {
                Ok(Async::Ready(value)) => {
                    // ok
                    assert_eq!(1, value);
                    None
                }
                Ok(Async::NotReady) => Some(rx),
                Err(_) => unreachable!(),
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

#[test]
fn changing_tx_task() {
    loom::fuzz(|| {
        let (mut tx, rx) = oneshot::channel::<i32>();

        thread::spawn(move || {
            drop(rx);
        });

        let tx = thread::spawn(move || {
            let t1 = block_on(futures::future::poll_fn(|| {
                Ok::<_, ()>(tx.poll_close().into())
            }))
            .unwrap();

            match t1 {
                Ok(Async::Ready(())) => None,
                Ok(Async::NotReady) => Some(tx),
                Err(_) => unreachable!(),
            }
        })
        .join()
        .unwrap();

        if let Some(mut tx) = tx {
            // Previous task parked, use a new task...
            block_on(futures::future::poll_fn(move || tx.poll_close())).unwrap();
        }
    });
}
