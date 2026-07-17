#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "time",
        feature = "sync",
        feature = "macros",
        feature = "test-util"
    )
))]

use tokio::time::{Duration, Instant};

#[tokio::test(start_paused = true)]
async fn test_start_paused() {
    let now = Instant::now();

    // Pause a few times w/ std sleep and ensure `now` stays the same
    for _ in 0..5 {
        std::thread::sleep(Duration::from_millis(1));
        assert_eq!(now, Instant::now());
    }
}
