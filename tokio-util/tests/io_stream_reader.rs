#![warn(rust_2018_idioms)]

use bytes::Bytes;
use tokio::io::AsyncReadExt;
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

#[cfg(feature = "rt")]
#[tokio::test]
async fn always_ready_empty_chunks_are_cooperative() {
    // Skipping successful empty chunks must yield without any I/O errors.
    let chunks = (0..256)
        .map(|_| Ok::<_, std::io::Error>(Bytes::new()))
        .chain(std::iter::once(Ok(Bytes::from_static(b"hello"))));
    let mut reader = StreamReader::new(futures::stream::iter(chunks));
    let mut output = [0; 5];
    let mut task = tokio_test::task::spawn(reader.read(&mut output));

    tokio_test::assert_pending!(task.poll());
    assert_eq!(task.await.unwrap(), 5);
    assert_eq!(&output, b"hello");
}

#[tokio::test]
async fn always_ready_empty_chunks_remain_unconstrained() {
    let chunks = (0..256)
        .map(|_| Ok::<_, std::io::Error>(Bytes::new()))
        .chain(std::iter::once(Ok(Bytes::from_static(b"hello"))));
    let mut reader = StreamReader::new(futures::stream::iter(chunks));
    let mut output = [0; 5];
    let mut task = tokio_test::task::spawn(tokio::task::unconstrained(reader.read(&mut output)));
    assert_eq!(tokio_test::assert_ready_ok!(task.poll()), 5);
    drop(task);
    assert_eq!(&output, b"hello");
}
