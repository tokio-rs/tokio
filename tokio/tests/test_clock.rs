#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "test-util"
    )
))]

use tokio::time::{self, Duration, Instant};

#[tokio::test]
async fn resume_lets_time_move_forward_instead_of_resetting_it() {
    let start = Instant::now();
    time::pause();
    time::advance(Duration::from_secs(10)).await;
    let advanced_by_ten_secs = Instant::now();
    assert!(advanced_by_ten_secs - start > Duration::from_secs(10));
    assert!(advanced_by_ten_secs - start < Duration::from_secs(11));
    time::resume();
    assert!(advanced_by_ten_secs < Instant::now());
    assert!(Instant::now() - advanced_by_ten_secs < Duration::from_secs(1));
}

#[tokio::test]
async fn can_pause_after_resume() {
    let start = Instant::now();
    time::pause();
    time::advance(Duration::from_secs(10)).await;
    time::resume();
    time::pause();
    time::advance(Duration::from_secs(10)).await;
    assert!(Instant::now() - start > Duration::from_secs(20));
    assert!(Instant::now() - start < Duration::from_secs(21));
}

#[tokio::test]
#[should_panic]
async fn freezing_time_while_frozen_panics() {
    time::pause();
    time::pause();
}

#[tokio::test]
#[should_panic]
async fn advancing_time_when_time_is_not_frozen_panics() {
    time::advance(Duration::from_secs(1)).await;
}

#[tokio::test]
#[should_panic]
async fn resuming_time_when_not_frozen_panics() {
    time::pause();
    time::resume();
    time::resume();
}

#[cfg(not(any(target_os = "wasi", target_os = "emscripten")))]
#[test]
fn advance_wakes_parked_driver() {
    use std::sync::mpsc;

    for enable_io in [false, true] {
        let (go_tx, go_rx) = mpsc::channel();
        let mut builder = tokio::runtime::Builder::new_current_thread();
        builder
            .enable_time()
            .start_paused(true)
            .on_thread_park(move || {
                let _ = go_tx.send(());
            });
        if enable_io {
            builder.enable_io();
        }
        let rt = builder.build().unwrap();

        rt.block_on(async {
            let start = Instant::now();
            let duration = Duration::from_secs(60);
            let (timer_complete_tx, timer_complete_rx) = mpsc::channel();
            let handle = tokio::runtime::Handle::current();
            // The blocking task inhibits auto-advance, so the driver parks in
            // real time. It stays alive until the timer fires, so its release
            // cannot be what wakes the driver.
            let worker = tokio::task::spawn_blocking(move || {
                go_rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| "runtime did not start parking")?;
                std::thread::sleep(Duration::from_millis(20));
                handle.block_on(time::advance(duration));
                timer_complete_rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| "parked timer did not complete before watchdog")
            });

            time::sleep(duration).await;
            let elapsed = start.elapsed();
            let _ = timer_complete_tx.send(());
            worker.await.unwrap().unwrap();
            assert_eq!(elapsed, duration);
        });
    }
}
