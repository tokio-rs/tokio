#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi")))] // Wasi does not support bind

use tokio::net::TcpListener;
use tokio::runtime;
use tokio_test::{assert_err, assert_pending, assert_ready, task};

use futures::task::{waker_ref, ArcWake};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::task::Context;

#[test]
fn tcp_doesnt_block() {
    let rt = rt();

    let listener = {
        let _enter = rt.enter();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

        listener.set_nonblocking(true).unwrap();

        TcpListener::from_std(listener).unwrap()
    };

    drop(rt);

    let mut task = task::spawn(async move {
        assert_err!(listener.accept().await);
    });

    assert_ready!(task.poll());
}

#[test]
fn drop_wakes() {
    let rt = rt();

    let listener = {
        let _enter = rt.enter();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

        listener.set_nonblocking(true).unwrap();

        TcpListener::from_std(listener).unwrap()
    };

    let mut task = task::spawn(async move {
        assert_err!(listener.accept().await);
    });

    assert_pending!(task.poll());

    drop(rt);

    assert!(task.is_woken());
    assert_ready!(task.poll());
}

struct RegistrationWaker {
    // Keep the registration alive through the waker to recreate the cycle from #3481.
    _listener: Arc<TcpListener>,
    dropped: Arc<AtomicBool>,
}

impl ArcWake for RegistrationWaker {
    fn wake_by_ref(_: &Arc<Self>) {}
}

impl Drop for RegistrationWaker {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::SeqCst);
    }
}

#[test]
fn shutdown_drops_waker_holding_registration() {
    let rt = rt();

    let listener = {
        let _enter = rt.enter();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

        listener.set_nonblocking(true).unwrap();

        Arc::new(TcpListener::from_std(listener).unwrap())
    };

    let dropped = Arc::new(AtomicBool::new(false));
    let task = Arc::new(RegistrationWaker {
        _listener: listener.clone(),
        dropped: dropped.clone(),
    });
    {
        let waker = waker_ref(&task);
        let mut cx = Context::from_waker(&waker);

        assert_pending!(listener.poll_accept(&mut cx));
    }

    drop(listener);
    drop(task);
    assert!(!dropped.load(Ordering::SeqCst));
    drop(rt);

    assert!(dropped.load(Ordering::SeqCst));
}

fn rt() -> runtime::Runtime {
    runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
