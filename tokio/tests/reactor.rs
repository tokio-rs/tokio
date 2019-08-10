#![feature(async_await)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "default")]

use tokio_reactor::Reactor;
use tokio_tcp::TcpListener;
use tokio_test::{assert_ok, assert_pending};

use futures_util::task::{waker_ref, ArcWake};
use std::future::Future;
use std::net::TcpStream;
use std::pin::Pin;
use std::sync::{mpsc, Arc, Mutex};
use std::task::Context;

struct Task<T> {
    future: Mutex<Pin<Box<T>>>,
}

impl<T: Send> ArcWake for Task<T> {
    fn wake_by_ref(_: &Arc<Self>) {
        // Do nothing...
    }
}

impl<T> Task<T> {
    fn new(future: T) -> Task<T> {
        Task {
            future: Mutex::new(Box::pin(future)),
        }
    }
}

#[test]
fn test_drop_on_notify() {
    // When the reactor receives a kernel notification, it notifies the
    // task that holds the associated socket. If this notification results in
    // the task being dropped, the socket will also be dropped.
    //
    // Previously, there was a deadlock scenario where the reactor, while
    // notifying, held a lock and the task being dropped attempted to acquire
    // that same lock in order to clean up state.
    //
    // To simulate this case, we create a fake executor that does nothing when
    // the task is notified. This simulates an executor in the process of
    // shutting down. Then, when the task handle is dropped, the task itself is
    // dropped.

    let mut reactor = assert_ok!(Reactor::new());
    let (addr_tx, addr_rx) = mpsc::channel();

    // Define a task that just drains the listener
    let task = Arc::new(Task::new(async move {
        let addr = assert_ok!("127.0.0.1:0".parse());
        // Create a listener
        let mut listener = assert_ok!(TcpListener::bind(&addr));

        // Send the address
        let addr = listener.local_addr().unwrap();
        addr_tx.send(addr).unwrap();

        loop {
            let _ = listener.accept().await;
        }
    }));

    let _enter = tokio_executor::enter().unwrap();

    tokio_reactor::with_default(&reactor.handle(), || {
        let waker = waker_ref(&task);
        let mut cx = Context::from_waker(&waker);
        assert_pending!(task.future.lock().unwrap().as_mut().poll(&mut cx));
    });

    // Get the address
    let addr = addr_rx.recv().unwrap();

    drop(task);

    // Establish a connection to the acceptor
    let _s = TcpStream::connect(&addr).unwrap();

    reactor.turn(None).unwrap();
}
