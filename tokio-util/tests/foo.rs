use bytes::Bytes;
use tokio::io::{AsyncReadExt, Result};
use tokio_util::io::StreamReader;
#[tokio::main]
async fn main() -> std::io::Result<()> {
    // Create a stream from an iterator.
    let stream = tokio::stream::iter(vec![
        Result::Ok(Bytes::from_static(&[0, 1, 2, 3])),
        Result::Ok(Bytes::from_static(&[4, 5, 6, 7])),
        Result::Ok(Bytes::from_static(&[8, 9, 10, 11])),
    ]);

    // Convert it to an AsyncRead.
    let mut read = StreamReader::new(stream);

    // Read five bytes from the stream.
    let mut buf = [0; 5];
    read.read_exact(&mut buf[..]).await?;
    assert_eq!(buf, [0, 1, 2, 3, 4]);

    // Read the rest of the current chunk.
    assert_eq!(read.read(&mut buf).await?, 3);
    assert_eq!(&buf[..3], [5, 6, 7]);

    // Read the next chunk.
    assert_eq!(read.read(&mut buf).await?, 4);
    assert_eq!(&buf[..4], [8, 9, 10, 11]);

    // We have now reached the end.
    assert_eq!(read.read(&mut buf).await?, 0);

    Ok(())
}
