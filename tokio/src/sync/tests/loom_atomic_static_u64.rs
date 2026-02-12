use crate::loom::sync::atomic::{Ordering::Relaxed, StaticAtomicU64};
use std::sync::Arc;

#[test]
fn static_atomic_u64_basic_operations() {
    loom::model(|| {
        let atomic = StaticAtomicU64::new(42);

        assert_eq!(atomic.load(Relaxed), 42);

        atomic.store(100, Relaxed);
        assert_eq!(atomic.load(Relaxed), 100);

        let prev = atomic.fetch_add(10, Relaxed);
        assert_eq!(prev, 100);
        assert_eq!(atomic.load(Relaxed), 110);

        let result = atomic.compare_exchange(110, 200, Relaxed, Relaxed);
        assert_eq!(result, Ok(110));
        assert_eq!(atomic.load(Relaxed), 200);

        let result = atomic.compare_exchange(150, 300, Relaxed, Relaxed);
        assert_eq!(result, Err(200));
        assert_eq!(atomic.load(Relaxed), 200);

        let result = atomic.compare_exchange_weak(200, 400, Relaxed, Relaxed);
        assert_eq!(result, Ok(200));
        assert_eq!(atomic.load(Relaxed), 400);

        let result = atomic.compare_exchange_weak(150, 500, Relaxed, Relaxed);
        assert_eq!(result, Err(400));
        assert_eq!(atomic.load(Relaxed), 400);
    });
}

#[test]
fn static_atomic_u64_concurrent_access() {
    loom::model(|| {
        let atomic = Arc::new(StaticAtomicU64::new(0));

        let thread1 = {
            let atomic = atomic.clone();
            loom::thread::spawn(move || {
                for _ in 0..10 {
                    atomic.fetch_add(1, Relaxed);
                }
            })
        };

        let thread2 = {
            let atomic = atomic.clone();
            loom::thread::spawn(move || {
                for _ in 0..5 {
                    atomic.fetch_add(2, Relaxed);
                }
            })
        };

        thread1.join().unwrap();
        thread2.join().unwrap();

        // Final value should be 10 (from thread1) + 5*2 = 10 (from thread2) = 20 total
        let final_value = atomic.load(Relaxed);
        assert_eq!(final_value, 20);
    });
}

#[test]
fn static_atomic_u64_const_init() {
    // Test that const initialization works
    const CONST_ATOMIC: StaticAtomicU64 = StaticAtomicU64::new(123);
    loom::model(|| {
        assert_eq!(CONST_ATOMIC.load(Relaxed), 123);
    });
}
