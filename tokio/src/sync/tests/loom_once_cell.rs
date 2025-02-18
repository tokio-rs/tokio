use crate::sync::OnceCell;
use loom::future::block_on;
use loom::sync::atomic::{AtomicUsize, Ordering};
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

        let th =
            thread::spawn(move || block_on(async { *cell2.get_or_init(|| async { 1 }).await }));

        let value = block_on(async { *cell.get_or_init(|| async { 2 }).await });
        assert!(value == 1 || value == 2);
        assert!(th.join().unwrap() == value);
    });
}

#[test]
fn get_or_try_init() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();

        let th =
            thread::spawn(move || block_on(async { *cell2.get_or_init(|| async { 1 }).await }));

        let res = block_on(async { cell.get_or_try_init(|| async { Err(()) }).await });
        assert!(matches!(res, Ok(&1) | Err(())));
        assert!(th.join().unwrap() == 1);
    });
}

#[test]
fn init_attempt_happens_before() {
    loom::model(|| {
        let cell = Arc::new(OnceCell::new());
        let cell2 = cell.clone();
        let cell3 = cell.clone();
        let atomic = Arc::new(AtomicUsize::new(0));
        let atomic2 = atomic.clone();
        let atomic3 = atomic.clone();

        async fn incr_and_fail(atomic: Arc<AtomicUsize>) -> Result<usize, usize> {
            let atomic_value = atomic.load(Ordering::Relaxed);
            atomic.fetch_add(1, Ordering::Relaxed);
            Err(atomic_value)
        }

        let th1 = thread::spawn(move || {
            block_on(async { cell2.get_or_try_init(|| incr_and_fail(atomic2)).await.err() })
        });
        let th2 = thread::spawn(move || {
            block_on(async { cell3.get_or_try_init(|| incr_and_fail(atomic3)).await.err() })
        });
        let atomic_value = cell.set(42).ok().map(|_| atomic.load(Ordering::Relaxed));

        assert!(matches!(
            (atomic_value, th1.join().unwrap(), th2.join().unwrap()),
            (None, Some(0), Some(1))
                | (None, Some(1), Some(0))
                | (Some(0), None, None)
                | (Some(1), Some(0), None)
                | (Some(1), None, Some(0))
                | (Some(2), Some(0), Some(1))
                | (Some(2), Some(1), Some(0)),
        ));
    });
}
