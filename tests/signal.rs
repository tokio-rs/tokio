#![cfg(unix)]

extern crate futures;
extern crate libc;
extern crate tokio_core;
extern crate tokio_signal;

use std::sync::mpsc::channel;
use std::sync::{Once, ONCE_INIT, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use futures::Future;
use futures::stream::Stream;
use tokio_core::Loop;
use tokio_signal::unix::Signal;

static INIT: Once = ONCE_INIT;
static mut LOCK: *mut Mutex<()> = 0 as *mut _;

fn lock() -> MutexGuard<'static, ()> {
    unsafe {
        INIT.call_once(|| {
            LOCK = Box::into_raw(Box::new(Mutex::new(())));
            let (tx, rx) = channel();
            thread::spawn(move || {
                let mut lp = Loop::new().unwrap();
                let handle = lp.handle();
                let _signal = lp.run(Signal::new(libc::SIGALRM, &handle)).unwrap();
                tx.send(()).unwrap();
                drop(lp.run(futures::empty::<(), ()>()));
            });
            rx.recv().unwrap();
        });
        (*LOCK).lock().unwrap()
    }
}

#[test]
fn simple() {
    let _lock = lock();

    let mut lp = Loop::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::new(libc::SIGUSR1, &handle)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    lp.run(signal.into_future()).ok().unwrap();
}

#[test]
fn notify_both() {
    let _lock = lock();

    let mut lp = Loop::new().unwrap();
    let handle = lp.handle();
    let signal1 = lp.run(Signal::new(libc::SIGUSR2, &handle)).unwrap();
    let signal2 = lp.run(Signal::new(libc::SIGUSR2, &handle)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR2), 0);
    }
    lp.run(signal1.into_future().join(signal2.into_future())).ok().unwrap();
}

#[test]
fn drop_then_get_a_signal() {
    let _lock = lock();

    let mut lp = Loop::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::new(libc::SIGUSR1, &handle)).unwrap();
    drop(signal);
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    let timeout = lp.handle().timeout(Duration::from_millis(1));
    lp.run(timeout.and_then(|t| t)).unwrap();
}

#[test]
fn twice() {
    let _lock = lock();

    let mut lp = Loop::new().unwrap();
    let handle = lp.handle();
    let signal = lp.run(Signal::new(libc::SIGUSR1, &handle)).unwrap();
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    let (num, signal) = lp.run(signal.into_future()).ok().unwrap();
    assert_eq!(num, Some(libc::SIGUSR1));
    unsafe {
        assert_eq!(libc::kill(libc::getpid(), libc::SIGUSR1), 0);
    }
    lp.run(signal.into_future()).ok().unwrap();
}
