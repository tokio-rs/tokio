use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use futures_01::future::Future as Future01;
use futures_util::compat::Future01CompatExt;

#[test]
fn can_run_01_futures() {
    let future_ran = Arc::new(AtomicBool::new(false));
    let ran = future_ran.clone();

    super::run(futures_01::future::lazy(move || {
        future_ran.store(true, Ordering::SeqCst);
        Ok::<(), ()>(())
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
fn can_spawn_std_futures() {
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

#[test]
fn tokio_01_timers_work() {
    let future1_ran = Arc::new(AtomicBool::new(false));
    let ran = future1_ran.clone();
    let future1 = futures_01::future::lazy(|| {
        let when = Instant::now() + Duration::from_millis(15);
        tokio_01::timer::Delay::new(when).map(move |_| when)
    })
    .map(move |when| {
        ran.store(true, Ordering::SeqCst);
        assert!(Instant::now() >= when);
    })
    .map_err(|_| panic!("timer should work"));

    let future2_ran = Arc::new(AtomicBool::new(false));
    let ran = future2_ran.clone();
    let future2 = async move {
        let when = Instant::now() + Duration::from_millis(10);
        tokio_01::timer::Delay::new(when).compat().await.unwrap();
        ran.store(true, Ordering::SeqCst);
        assert!(Instant::now() >= when);
    };

    super::run(futures_01::future::lazy(move || {
        tokio_02::spawn(future2);
        tokio_01::spawn(future1);
        Ok(())
    }));
    assert!(future1_ran.load(Ordering::SeqCst));
    assert!(future2_ran.load(Ordering::SeqCst));
}

#[test]
fn block_on_01_timer() {
    let mut rt = super::Runtime::new().unwrap();
    let when = Instant::now() + Duration::from_millis(10);
    rt.block_on(tokio_01::timer::Delay::new(when)).unwrap();
    assert!(Instant::now() >= when);
}

#[test]
fn block_on_std_01_timer() {
    let mut rt = super::Runtime::new().unwrap();
    let when = Instant::now() + Duration::from_millis(10);
    rt.block_on_std(async move {
        tokio_01::timer::Delay::new(when).compat().await.unwrap();
    });
    assert!(Instant::now() >= when);
}

#[test]
fn block_on_01_spawn() {
    let mut rt = super::Runtime::new().unwrap();
    // other tests assert that spawned 0.1 tasks actually *run*, all we care
    // is that we're able to spawn it successfully.
    rt.block_on(futures_01::future::lazy(|| {
        tokio_01::spawn(futures_01::future::lazy(|| Ok(())))
    }))
    .unwrap();
}

#[test]
fn block_on_std_01_spawn() {
    let mut rt = super::Runtime::new().unwrap();
    // other tests assert that spawned 0.1 tasks actually *run*, all we care
    // is that we're able to spawn it successfully.
    rt.block_on_std(async { tokio_01::spawn(futures_01::future::lazy(|| Ok(()))) });
}
