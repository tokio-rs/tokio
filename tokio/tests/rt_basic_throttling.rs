#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::time;
use tokio_test::assert_ok;

use std::time::{Duration, Instant};

const MAX_THROTTLING: Duration = Duration::from_millis(200);

// One time frame duration
//
// Depending on when the `delay` is created, it might be fired either:
//
// - at the beginning of time frame tf2
//
//      tf1   tf2   tf3
//    |--:--|--:--|--:--|...
//      ^---x
//      <-dur->
//
//
// - or, at the beginning of tme frame tf3
//
//      tf1   tf2   tf3
//    |--:--|--:--|--:--|...
//        ^-------x
//        <-dur->

#[test]
fn delay_at_root_one_time_frame() {
    let mut rt = rt();

    let dur = MAX_THROTTLING;

    let elapsed = rt.block_on(async move {
        let now = Instant::now();
        time::delay_for(dur).await;
        now.elapsed()
    });

    // delay can be fired earlier if it was created
    // before half of the time frame
    assert!(elapsed + (MAX_THROTTLING / 2) >= dur);
    // delay is created during the first time frame
    // and must be fired at the beginning of the next time frame
    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(elapsed < dur + MAX_THROTTLING);
}

#[test]
fn delay_in_spawn_one_time_frame() {
    let mut rt = rt();

    let dur = MAX_THROTTLING;

    let elapsed = rt.block_on(async move {
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let now = Instant::now();
            time::delay_for(dur).await;
            assert_ok!(tx.send(now.elapsed()));
        });

        assert_ok!(rx.await)
    });

    // delay can be fired earlier if it was created
    // before half of the time frame
    assert!(elapsed + (MAX_THROTTLING / 2) >= dur);
    // delay is created during the first time frame
    // and must be fired at the beginning of the next time frame
    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(elapsed < dur + MAX_THROTTLING);
}

// One time frame + 2/3 (i.e. more than a half) duration
//
// Whenever the `delay` is created in tf1, it will always be fired
// at the beginning of time frame tf2:
//
//      tf1   tf2   tf3
//    |--:--|--:--|--:--|...
//      ^---------x
//      <--dur.-->
//
//      tf1   tf2   tf3
//    |--:--|--:--|--:--|...
//        ^-------x
//        <--dur.-->

#[test]
fn delay_at_root_one_time_frame_2_3() {
    let mut rt = rt();

    let dur = MAX_THROTTLING + MAX_THROTTLING * 2 / 3;

    let _elapsed = rt.block_on(async move {
        let now = Instant::now();
        time::delay_for(dur).await;
        now.elapsed()
    });

    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(_elapsed >= dur);
    // delay is created during the first time frame
    // and must be fired at the beginning of the next time frame
    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(_elapsed < dur + MAX_THROTTLING);
}

#[test]
fn delay_in_spawn_one_time_frame_2_3() {
    let mut rt = rt();

    let dur = MAX_THROTTLING + MAX_THROTTLING * 2 / 3;

    let _elapsed = rt.block_on(async move {
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let now = Instant::now();
            time::delay_for(dur).await;
            assert_ok!(tx.send(now.elapsed()));
        });

        assert_ok!(rx.await)
    });

    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(_elapsed >= dur);
    // delay is created during the first time frame
    // and must be fired at the beginning of the next time frame
    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(_elapsed < dur + MAX_THROTTLING);
}

// Two time frames duration
//
// Depending on when the `delay` is created, it might be fired either:
//
// - at the beginning of time frame tf3
//
//      tf1   tf2   tf3   tf4
//    |--:--|--:--|--:--|--:--|...
//      ^---------x
//      <----dur---->
//
// - or, at the beginning of tme frame tf4
//
//      tf1   tf2   tf3   tf4
//    |--:--|--:--|--:--|--:--|...
//        ^-------------x
//        <----dur---->

#[test]
fn delay_at_root_two_time_frames() {
    let mut rt = rt();

    let dur = MAX_THROTTLING * 2;

    let elapsed = rt.block_on(async move {
        let now = Instant::now();
        time::delay_for(dur).await;
        now.elapsed()
    });

    // delay can be fired earlier if it was created
    // before half of the time frame
    assert!(elapsed + (MAX_THROTTLING / 2) >= dur);
    // delay is created during the first time frame
    // and must be fired after the end of next time frame
    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(elapsed < dur + MAX_THROTTLING);
}

#[test]
fn delay_in_spawn_two_time_frames() {
    let mut rt = rt();

    let dur = MAX_THROTTLING * 2;

    let elapsed = rt.block_on(async move {
        let (tx, rx) = oneshot::channel();

        tokio::spawn(async move {
            let now = Instant::now();
            time::delay_for(dur).await;
            assert_ok!(tx.send(now.elapsed()));
        });

        assert_ok!(rx.await)
    });

    // delay can be fired earlier if it was created
    // before half of the time frame
    assert!(elapsed + (MAX_THROTTLING / 2) >= dur);
    // delay is created during the first time frame
    // and must be fired after the end of next time frame
    // Discard this check for macos as CI behavior regarding time is not consistent
    #[cfg(not(target_os = "macos"))]
    assert!(elapsed < dur + MAX_THROTTLING);
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .max_throttling(MAX_THROTTLING)
        .build()
        .unwrap()
}
