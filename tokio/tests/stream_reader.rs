#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use bytes::Bytes;
use tokio::io::{stream_reader, AsyncReadExt};
use tokio::stream::iter;

#[tokio::test]
async fn test_stream_reader() -> std::io::Result<()> {
    let stream = iter(vec![
        Ok(Bytes::from_static(&[])),
        Ok(Bytes::from_static(&[0, 1, 2, 3])),
        Ok(Bytes::from_static(&[])),
        Ok(Bytes::from_static(&[4, 5, 6, 7])),
        Ok(Bytes::from_static(&[])),
        Ok(Bytes::from_static(&[8, 9, 10, 11])),
        Ok(Bytes::from_static(&[])),
    ]);

    let mut read = stream_reader(stream);

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
