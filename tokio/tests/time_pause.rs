#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use rand::SeedableRng;
use rand::{rngs::StdRng, Rng};
use tokio::time::{self, Duration, Instant};
use tokio_test::assert_err;

#[tokio::test]
async fn pause_time_in_main() {
    tokio::time::pause();
}

#[tokio::test]
async fn pause_time_in_task() {
    let t = tokio::spawn(async {
        tokio::time::pause();
    });

    t.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
#[should_panic]
async fn pause_time_in_main_threads() {
    tokio::time::pause();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 1)]
async fn pause_time_in_spawn_threads() {
    let t = tokio::spawn(async {
        tokio::time::pause();
    });

    assert_err!(t.await);
}

#[test]
fn paused_time_is_deterministic() {
    let run_1 = paused_time_stress_run();
    let run_2 = paused_time_stress_run();

    assert_eq!(run_1, run_2);
}

#[tokio::main(flavor = "current_thread", start_paused = true)]
async fn paused_time_stress_run() -> Vec<Duration> {
    let mut rng = StdRng::seed_from_u64(1);

    let mut times = vec![];
    let start = Instant::now();
    for _ in 0..10_000 {
        let sleep = rng.gen_range(Duration::from_secs(0)..Duration::from_secs(1));
        time::sleep(sleep).await;
        times.push(start.elapsed());
    }

    times
}
