use futures_util::{Sink, SinkExt, StreamExt};
use std::io;
use std::pin::Pin;
use std::time::Duration;
use tokio::time::Instant;
use tokio_test::sink_mock::SinkMockBuilder;
use tokio_test::stream_mock::StreamMockBuilder;

#[tokio::test]
async fn test_sink_mock_empty() {
    let mut sink_mock = SinkMockBuilder::<u32, io::Error>::new().build();

    sink_mock.flush().await.unwrap();
    sink_mock.close().await.unwrap();
}

#[tokio::test]
async fn test_sink_mock_items() {
    let mut sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .send(1)
        .send(2)
        .build();

    sink_mock.send(1).await.unwrap();
    sink_mock.send(2).await.unwrap();
    sink_mock.close().await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn test_sink_mock_wait() {
    let mut sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .send(1)
        .wait(Duration::from_millis(300))
        .send(2)
        .wait(Duration::from_millis(300))
        .build();

    sink_mock.feed(1).await.unwrap();
    let start = Instant::now();
    sink_mock.feed(2).await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(300));

    let start = Instant::now();
    sink_mock.flush().await.unwrap();
    assert!(start.elapsed() >= Duration::from_millis(300));
}

#[tokio::test]
async fn test_sink_mock_error() {
    let mut sink_mock = SinkMockBuilder::new()
        .error(io::Error::other("poll_ready"))
        .send(1)
        .error(io::Error::other("poll_flush"))
        .error(io::Error::other("poll_close"))
        .build();

    // `send` calls `poll_ready`, then `start_send` and `poll_flush`.
    let error = sink_mock.send(1).await.unwrap_err();
    assert_eq!(error.to_string(), "poll_ready");
    let error = sink_mock.send(1).await.unwrap_err();
    assert_eq!(error.to_string(), "poll_flush");
    let error = sink_mock.close().await.unwrap_err();
    assert_eq!(error.to_string(), "poll_close");
}

#[tokio::test(start_paused = true)]
async fn test_sink_mock_forward_from_stream_mock() {
    let stream_mock = StreamMockBuilder::new()
        .next(Ok(1))
        .wait(Duration::from_millis(300))
        .next(Ok(2))
        .build();
    let sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .send(1)
        .send(2)
        .build();

    stream_mock.forward(sink_mock).await.unwrap();
}

#[tokio::test]
#[should_panic(expected = "SinkMock received unexpected item 2, expected 1")]
async fn test_sink_mock_unexpected_item() {
    let mut sink_mock = SinkMockBuilder::<_, io::Error>::new().send(1).build();

    let _ = sink_mock.send(2).await;
}

#[tokio::test]
#[should_panic(expected = "SinkMock received unexpected item 3, no more items were expected")]
async fn test_sink_mock_item_after_all_actions_consumed() {
    let mut sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .send(1)
        .send(2)
        .build();

    sink_mock.send(1).await.unwrap();
    sink_mock.send(2).await.unwrap();
    let _ = sink_mock.send(3).await;
}

#[test]
#[should_panic(expected = "SinkMock received item 1 before `poll_ready` returned `Ready(Ok(()))`")]
fn test_sink_mock_start_send_before_poll_ready() {
    let mut sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .wait(Duration::from_millis(300))
        .send(1)
        .build();

    let _ = Pin::new(&mut sink_mock).start_send(1);
}

#[tokio::test]
#[should_panic(expected = "SinkMock was dropped before all actions were consumed")]
async fn test_sink_mock_drop_without_consuming_all() {
    let sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .send(1)
        .send(2)
        .build();
    drop(sink_mock);
}

#[tokio::test]
#[should_panic(expected = "SinkMock was dropped before all actions were consumed")]
async fn test_sink_mock_drop_without_returning_error() {
    let sink_mock = SinkMockBuilder::<u32, _>::new()
        .error(io::Error::other("never returned"))
        .build();
    drop(sink_mock);
}

#[tokio::test]
async fn test_sink_mock_drop_with_remaining_wait() {
    let sink_mock = SinkMockBuilder::<u32, io::Error>::new()
        .wait(Duration::from_millis(300))
        .build();
    drop(sink_mock);
}

#[tokio::test]
#[should_panic(expected = "test panic was not masked")]
async fn test_sink_mock_drop_during_panic_doesnt_mask_panic() {
    let _sink_mock = SinkMockBuilder::<_, io::Error>::new()
        .send(1)
        .send(2)
        .build();
    panic!("test panic was not masked");
}
