use crate::sync::SetOnce;

use loom::future::block_on;
use loom::sync::atomic::AtomicU32;
use loom::thread;
use std::sync::atomic::Ordering;
use std::sync::Arc;

struct Foo {
    pub num_drops: Arc<AtomicU32>,
}

impl Foo {
    pub fn new(num_drops: Arc<AtomicU32>) -> Self {
        Self { num_drops }
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
        let set_once = Arc::new(SetOnce::new());
        let set_once_clone = Arc::clone(&set_once);

        let drop_counter = Arc::new(AtomicU32::new(0));
        let counter_cl = Arc::clone(&drop_counter);

        let thread = thread::spawn(move || set_once_clone.set(Foo::new(counter_cl)).is_ok());

        let foo = Foo::new(Arc::clone(&drop_counter));

        let set = set_once.set(foo).is_ok();
        let res = thread.join().unwrap();

        drop(set_once);

        assert_eq!(drop_counter.load(Ordering::Acquire), 2);
        assert!(res || set);
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
