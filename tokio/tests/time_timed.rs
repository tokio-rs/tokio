use tokio::time::{self, timed, Duration};

#[tokio::test(start_paused = true)]
async fn accuracy() {
    let durations = vec![
        Duration::from_millis(50),
        Duration::from_millis(100),
        Duration::from_millis(200),
    ];
    for expected in durations {
        let ((), elapsed) = timed(time::sleep(expected)).await;
        assert_eq!(elapsed, expected);
    }
}

#[tokio::test(start_paused = true)]
async fn immediate_future() {
    let ((), elapsed) = timed(std::future::ready(())).await;
    assert_eq!(elapsed, Duration::from_millis(0));
}
