use crate::sync::OnceCell;
use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;

const VALUE: usize = 42;

#[test]
fn zst() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();

        let th = thread::spawn(move || {
            block_on(async {
                cell2.wait_initialized().await;
                assert_eq!(cell2.initialized(), true);
            });
        });

        cell.set(()).unwrap();
        th.join().unwrap();
    });
}

#[test]
fn wait_initialized() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();

        let th = thread::spawn(move || {
            block_on(async {
                assert_eq!(*cell2.wait_initialized().await, VALUE);
            });
        });

        cell.set(VALUE).unwrap();
        th.join().unwrap();
    });
}
