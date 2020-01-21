use std::time::Duration;
use tokio::stream::{Stream, StreamExt};
use tokio::sync::mpsc;
use tokio::time::delay_for;

#[tokio::test]
async fn basic_usage() {
    #[derive(Debug, PartialEq)]
    struct TimedOut;

    let actual = stream()
        .timeout(Duration::from_millis(15))
        .map(|item| item.map_err(|_| TimedOut))
        .collect::<Vec<_>>()
        .await;

    //              5ms    10ms   15ms                  20ms
    let expected = [Ok(1), Ok(2), Err(TimedOut), Ok(3), Err(TimedOut), Ok(4)];
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn no_timeout() {
    assert!(
        stream()
            .timeout(Duration::from_millis(50))
            .all(|item| item.is_ok())
            .await
    )
}

fn stream() -> impl Stream<Item = u32> {
    let (tx, rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut delay = 5;

        for n in 1..=4 {
            delay_for(Duration::from_millis(delay)).await;
            tx.send(n).expect("send");
            delay += 5;
        }
    });

    rx
}
