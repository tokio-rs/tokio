#![warn(rust_2018_idioms)]
use tokio::io::AsyncReadExt;
use tokio_util::io::SyncReadIntoAsyncRead;

#[tokio::test]
async fn test_sync_read_to_async_read() -> std::io::Result<()> {
    let buffer = std::io::Cursor::new(bytes::Bytes::from_static(&[0, 1, 2, 3, 4, 5, 6, 7]));
    let sync_reader = std::io::BufReader::new(buffer);
    let mut async_reader =
        SyncReadIntoAsyncRead::<_, bytes::BytesMut>::new_with_reader(sync_reader);

    let mut buf = [0; 5];
    async_reader.read_exact(&mut buf).await?;
    assert_eq!(buf, [0, 1, 2, 3, 4]);

    assert_eq!(async_reader.read(&mut buf).await?, 3);
    assert_eq!(&buf[..3], [5, 6, 7]);

    assert_eq!(async_reader.read(&mut buf).await?, 0);

    Ok(())
}
