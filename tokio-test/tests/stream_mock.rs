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

// Ref https://github.com/tokio-rs/tokio/issues/7445
#[tokio::test]
async fn test_out_of_order_read_write_sleep_on_mocks_doesnt_hang() {
    let socket = tokio_test::io::Builder::new()
        .wait(Duration::from_millis(10))
        .write([0].as_slice())
        .read([0].as_slice())
        .build();

    let (mut recv, mut send) = tokio::io::split(socket);
    let read_task = tokio::spawn(async move {
        tokio::io::AsyncReadExt::read_u8(&mut recv)
            .await
            .expect("expected read to complete successfully")
    });
    tokio::io::AsyncWriteExt::write_u8(&mut send, 0)
        .await
        .expect("expected write to complete successfully");

    read_task
        .await
        .expect("read_task did not complete successfully");
}
