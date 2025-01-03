use tokio_stream::{self as stream, StreamExt};

mod support {
    pub(crate) mod mpsc;
}

#[tokio::test]
async fn partition() {
    let stream = stream::iter(0..4);
    let (mut even, mut odd) = stream.partition(|v| v % 2 == 0);
    assert_eq!(Some(0), even.next().await);
    assert_eq!(Some(1), odd.next().await);
    assert_eq!(Some(2), even.next().await);
    assert_eq!(Some(3), odd.next().await);
    assert_eq!(None, even.next().await);
    assert_eq!(None, odd.next().await);
}

#[tokio::test]
async fn partition_buffers() {
    let stream = stream::iter(0..4);
    let (mut even, mut odd) = stream.partition(|v| v % 2 == 0);
    assert_eq!(Some(1), odd.next().await);
    assert_eq!(Some(3), odd.next().await);
    assert_eq!(None, odd.next().await);
    assert_eq!(Some(0), even.next().await);
    assert_eq!(Some(2), even.next().await);
    assert_eq!(None, odd.next().await);
}
