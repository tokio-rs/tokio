extern crate tokio_core;
extern crate env_logger;
extern crate futures;

use std::time::Duration;
use std::sync::mpsc;

use futures::Future;
use tokio_core::reactor::Core;

#[test]
fn simple() {
    drop(env_logger::init());
    let mut lp = Core::new().unwrap();

    let (tx1, rx1) = futures::oneshot();
    let (tx2, rx2) = futures::oneshot();
    lp.handle().spawn(futures::lazy(|| {
        tx1.complete(1);
        Ok(())
    }));
    lp.remote().spawn(|_| {
        futures::lazy(|| {
            tx2.complete(2);
            Ok(())
        })
    });

    assert_eq!(lp.run(rx1.join(rx2)).unwrap(), (1, 2));
}

#[test]
fn simple_core_poll() {
    drop(env_logger::init());
    let mut lp = Core::new().unwrap();

    let (tx, rx) = mpsc::channel();
    let (tx1, tx2) = (tx.clone(), tx.clone());

    lp.turn(Some(Duration::new(0, 0)));
    lp.handle().spawn(futures::lazy(move || {
        tx1.send(1).unwrap();
        Ok(())
    }));
    lp.turn(Some(Duration::new(0, 0)));
    lp.handle().spawn(futures::lazy(move || {
        tx2.send(2).unwrap();
        Ok(())
    }));
    assert_eq!(rx.try_recv().unwrap(), 1);
    assert!(rx.try_recv().is_err());
    lp.turn(Some(Duration::new(0, 0)));
    assert_eq!(rx.try_recv().unwrap(), 2);
}

#[test]
fn spawn_in_poll() {
    drop(env_logger::init());
    let mut lp = Core::new().unwrap();

    let (tx1, rx1) = futures::oneshot();
    let (tx2, rx2) = futures::oneshot();
    let remote = lp.remote();
    lp.handle().spawn(futures::lazy(move || {
        tx1.complete(1);
        remote.spawn(|_| {
            futures::lazy(|| {
                tx2.complete(2);
                Ok(())
            })
        });
        Ok(())
    }));

    assert_eq!(lp.run(rx1.join(rx2)).unwrap(), (1, 2));
}
