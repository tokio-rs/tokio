#![warn(rust_2018_idioms)]

use futures_util::SinkExt;
use std::io::{Error, ErrorKind};
use tokio::io::AsyncWriteExt;
use tokio_util::io::SinkWriter;
use tokio_util::sync::PollSender;

#[tokio::test]
async fn test_sink_writer() -> Result<(), Error> {
    // Construct a channel pair to send data across and wrap a pollable sink.
    // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
    // as its error type.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(4);
    let mut writer =
        SinkWriter::new(PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::Other)));

    // Write data to our interface...
    let data: [u8; 4] = [1, 2, 3, 4];
    let _ = writer.write(&data).await?;

    // ... and receive it.
    let mut received = Vec::new();
    for _ in 0..4 {
        if let Some(b) = rx.recv().await {
            received.push(b);
        }
    }
    assert_eq!(&data, received.as_slice());

    Ok(())
}
