extern crate tokio_core;
extern crate env_logger;
extern crate futures;

use futures::Future;
use tokio_core::reactor::Core;

#[test]
fn simple() {
    drop(env_logger::init());
    let mut lp = Core::new().unwrap();

    let (tx1, rx1) = futures::oneshot();
    let (tx2, rx2) = futures::oneshot();
    lp.pin().spawn(futures::lazy(|| {
        tx1.complete(1);
        Ok(())
    }));
    lp.handle().spawn(|_| {
        futures::lazy(|| {
            tx2.complete(2);
            Ok(())
        })
    });

    assert_eq!(lp.run(rx1.join(rx2)).unwrap(), (1, 2));
}

#[test]
fn spawn_in_poll() {
    drop(env_logger::init());
    let mut lp = Core::new().unwrap();

    let (tx1, rx1) = futures::oneshot();
    let (tx2, rx2) = futures::oneshot();
    let handle = lp.handle();
    lp.pin().spawn(futures::lazy(move || {
        tx1.complete(1);
        handle.spawn(|_| {
            futures::lazy(|| {
                tx2.complete(2);
                Ok(())
            })
        });
        Ok(())
    }));

    assert_eq!(lp.run(rx1.join(rx2)).unwrap(), (1, 2));
}
