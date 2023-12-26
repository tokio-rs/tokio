use futures_util::StreamExt;
use std::time::Duration;
use tokio_test::stream_mock::StreamMockBuilder;

#[tokio::test]
async fn test_stream_mock_empty() {
    let mut stream_mock = StreamMockBuilder::<u32>::new().build();

    assert_eq!(stream_mock.next().await, None);
    assert_eq!(stream_mock.next().await, None);
}

#[tokio::test]
async fn test_stream_mock_items() {
    let mut stream_mock = StreamMockBuilder::new().next(1).next(2).build();

    assert_eq!(stream_mock.next().await, Some(1));
    assert_eq!(stream_mock.next().await, Some(2));
    assert_eq!(stream_mock.next().await, None);
}

#[tokio::test]
async fn test_stream_mock_wait() {
    let mut stream_mock = StreamMockBuilder::new()
        .next(1)
        .wait(Duration::from_millis(300))
        .next(2)
        .build();

    assert_eq!(stream_mock.next().await, Some(1));
    let start = std::time::Instant::now();
    assert_eq!(stream_mock.next().await, Some(2));
    let elapsed = start.elapsed();
    assert!(elapsed >= Duration::from_millis(300));
    assert_eq!(stream_mock.next().await, None);
}

#[tokio::test]
#[should_panic(expected = "StreamMock was dropped before all actions were consumed")]
async fn test_stream_mock_drop_without_consuming_all() {
    let stream_mock = StreamMockBuilder::new().next(1).next(2).build();
    drop(stream_mock);
}

#[tokio::test]
#[should_panic(expected = "test panic was not masked")]
async fn test_stream_mock_drop_during_panic_doesnt_mask_panic() {
    let _stream_mock = StreamMockBuilder::new().next(1).next(2).build();
    panic!("test panic was not masked");
}
