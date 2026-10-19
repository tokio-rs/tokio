#![warn(rust_2018_idioms)]

use bytes::Bytes;
use tokio::io::{AsyncBufReadExt, AsyncReadExt};
use tokio_stream::iter;
use tokio_util::io::StreamReader;

#[tokio::test]
async fn test_stream_reader() -> std::io::Result<()> {
    let stream = iter(vec![
        std::io::Result::Ok(Bytes::from_static(&[])),
        Ok(Bytes::from_static(&[0, 1, 2, 3])),
        Ok(Bytes::from_static(&[])),
        Ok(Bytes::from_static(&[4, 5, 6, 7])),
        Ok(Bytes::from_static(&[])),
        Ok(Bytes::from_static(&[8, 9, 10, 11])),
        Ok(Bytes::from_static(&[])),
    ]);

    let mut read = StreamReader::new(stream);

    let mut buf = [0; 5];
    read.read_exact(&mut buf).await?;
    assert_eq!(buf, [0, 1, 2, 3, 4]);

    assert_eq!(read.read(&mut buf).await?, 3);
    assert_eq!(&buf[..3], [5, 6, 7]);

    assert_eq!(read.read(&mut buf).await?, 4);
    assert_eq!(&buf[..4], [8, 9, 10, 11]);

    assert_eq!(read.read(&mut buf).await?, 0);

    Ok(())
}

#[tokio::test]
async fn test_stream_reader_does_not_poll_after_eof() -> std::io::Result<()> {
    // the first poll of this stream will return `Poll::Ready(None)`,
    // and the second poll will panic
    let stream = futures::stream::unfold((), |_| async { None::<(std::io::Result<Bytes>, ())> });
    let read = StreamReader::new(stream);
    tokio::pin!(read);
    let mut buf = [0; 1];

    // the first poll hits the inner stream,
    // and the inner stream returns `Poll::Ready(None)`.
    assert_eq!(read.read(&mut buf).await?, 0);
    // the second poll doesn't hit the inner stream,
    // so this `.read()` doesn't panic.
    assert_eq!(read.read(&mut buf).await?, 0);

    Ok(())
}

#[tokio::test]
async fn test_stream_reader_yields_after_many_empty_chunks() -> std::io::Result<()> {
    let empty = std::iter::repeat_with(|| Ok::<Bytes, std::io::Error>(Bytes::new())).take(256);
    let data = std::iter::once(Ok(Bytes::from_static(b"x")));
    let stream = futures::stream::iter(empty.chain(data));
    let mut read = StreamReader::new(stream);

    let mut read_task = tokio_test::task::spawn(read.fill_buf());

    // Each poll consumes at most 128 empty chunks and must schedule another poll.
    tokio_test::assert_pending!(read_task.poll());
    assert!(read_task.is_woken());
    tokio_test::assert_pending!(read_task.poll());
    assert!(read_task.is_woken());

    assert_eq!(tokio_test::assert_ready!(read_task.poll())?, b"x");
    Ok(())
}
