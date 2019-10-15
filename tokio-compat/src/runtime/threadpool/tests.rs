use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

#[test]
fn can_run_01_futures() {
    let future_ran = Arc::new(AtomicBool::new(false));
    let ran = future_ran.clone();
    super::run(futures_01::future::lazy(move || {
        future_ran.store(true, Ordering::SeqCst);
        Ok(())
    }));
    assert!(ran.load(Ordering::SeqCst));
}

#[test]
fn can_spawn_01_futures() {
    let future_ran = Arc::new(AtomicBool::new(false));
    let ran = future_ran.clone();
    super::run(futures_01::future::lazy(move || {
        tokio_01::spawn(futures_01::future::lazy(move || {
            future_ran.store(true, Ordering::SeqCst);
            Ok(())
        }));
        Ok(())
    }));
    assert!(ran.load(Ordering::SeqCst));
}

#[test]
fn can_spawn_03_futures() {
    let future_ran = Arc::new(AtomicBool::new(false));
    let ran = future_ran.clone();
    super::run(futures_01::future::lazy(move || {
        tokio_02::spawn(async move {
            future_ran.store(true, Ordering::SeqCst);
        });
        Ok(())
    }));
    assert!(ran.load(Ordering::SeqCst));
}
