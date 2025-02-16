use crate::sync::OnceCell;
use loom::future::block_on;
use loom::sync::Arc;
use loom::thread;

#[test]
fn zst() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();

        let th = thread::spawn(move || {
            block_on(async {
                cell2.wait_initialized().await;
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
                assert_eq!(*cell2.wait_initialized().await, 42);
            });
        });

        cell.set(42).unwrap();
        th.join().unwrap();
    });
}

#[test]
fn get_or_init() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();

        let th1 =
            thread::spawn(move || block_on(async { *cell2.get_or_init(|| async { 1 }).await }));

        let value = block_on(async { *cell.get_or_init(|| async { 2 }).await });
        assert!(value == 1 || value == 2);
        assert!(th1.join().unwrap() == value);
    });
}

#[test]
fn get_or_try_init() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();

        let th1 =
            thread::spawn(move || block_on(async { *cell2.get_or_init(|| async { 1 }).await }));

        let res = block_on(async { cell.get_or_try_init(|| async { Err(()) }).await });
        assert!(matches!(res, Ok(&1) | Err(())));
        assert!(th1.join().unwrap() == 1);
    });
}
