use crate::sync::SetOnce;

use loom::future::block_on;
use loom::sync::atomic::AtomicU32;
use loom::thread;
use std::sync::atomic::Ordering;
use std::sync::Arc;

struct Foo {
    pub num_drops: Arc<AtomicU32>,
    pub ok_attempts: Arc<AtomicU32>,
}

impl Foo {
    pub fn new(num_drops: Arc<AtomicU32>, ok_attempts: Arc<AtomicU32>) -> Self {
        Self {
            num_drops,
            ok_attempts,
        }
    }

    pub fn set_ok(&self) {
        self.ok_attempts.fetch_add(1, Ordering::Release);
    }
}

impl Drop for Foo {
    fn drop(&mut self) {
        self.num_drops.fetch_add(1, Ordering::Release);
    }
}

#[test]
fn set_once_drop_test() {
    loom::model(|| {
        let tx = Arc::new(SetOnce::new());
        let rx = Arc::clone(&tx);

        let drop_counter = Arc::new(AtomicU32::new(0));
        let counter_cl = Arc::clone(&drop_counter);

        let ok_attempts = Arc::new(AtomicU32::new(0));
        let ok_cl = Arc::clone(&ok_attempts);

        let thread = thread::spawn(move || {
            let foo = Arc::new(Foo::new(counter_cl, ok_cl));
            let cl = Arc::clone(&foo);

            if rx.set(foo).is_ok() {
                cl.set_ok();
            }
        });

        let foo = Arc::new(Foo::new(
            Arc::clone(&drop_counter),
            Arc::clone(&ok_attempts),
        ));

        let cl = Arc::clone(&foo);

        if tx.set(foo).is_ok() {
            cl.set_ok();
            drop(cl);
        }

        thread.join().unwrap();

        drop(tx);

        assert_eq!(drop_counter.load(Ordering::Acquire), 2);
        assert_eq!(ok_attempts.load(Ordering::Acquire), 1);
    });
}

#[test]
fn set_once_wait_test() {
    loom::model(|| {
        let tx = Arc::new(SetOnce::new());
        let rx_one = tx.clone();
        let rx_two = tx.clone();

        let thread = thread::spawn(move || {
            assert!(rx_one.set(2).is_ok());
        });

        block_on(async {
            assert_eq!(*rx_two.wait().await, 2);
        });

        thread.join().unwrap();
    });
}
