use crate::sync::watch;

use loom::future::block_on;
use loom::thread;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn smoke() {
    loom::model(|| {
        let (tx, mut rx1) = watch::channel(1);
        let mut rx2 = rx1.clone();
        let mut rx3 = rx1.clone();
        let mut rx4 = rx1.clone();
        let mut rx5 = rx1.clone();

        let th = thread::spawn(move || {
            tx.send(2).unwrap();
        });

        block_on(rx1.changed()).unwrap();
        assert_eq!(*rx1.borrow(), 2);

        block_on(rx2.changed()).unwrap();
        assert_eq!(*rx2.borrow(), 2);

        block_on(rx3.changed()).unwrap();
        assert_eq!(*rx3.borrow(), 2);

        block_on(rx4.changed()).unwrap();
        assert_eq!(*rx4.borrow(), 2);

        block_on(rx5.changed()).unwrap();
        assert_eq!(*rx5.borrow(), 2);

        th.join().unwrap();
    })
}

#[test]
fn wait_for_test() {
    loom::model(move || {
        let (tx, mut rx) = watch::channel(false);

        let tx_arc = Arc::new(tx);
        let tx1 = tx_arc.clone();
        let tx2 = tx_arc.clone();
        
        let th1 = thread::spawn(move || {
            for _ in 0..10 {
                tx1.send(false).unwrap();
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        let th2 = thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
           tx2.send(true).unwrap();
        });

        let result = block_on(rx.wait_for(|x| *x));
        assert_eq!(result.unwrap(), true);

        th1.join().unwrap();
        th2.join().unwrap();
    });
}