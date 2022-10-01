#![warn(rust_2018_idioms)]

use futures_util::SinkExt;
use std::io::{self, Error, ErrorKind};
use tokio::io::AsyncWriteExt;
use tokio_util::io::SinkWriter;
use tokio_util::sync::PollSender;

#[tokio::test]
async fn test_sink_writer() -> Result<(), Error> {
    // Construct a channel pair to send data across and wrap a pollable sink.
    // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
    // as its error type.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Vec<u8>>(1);
    let mut writer = SinkWriter::new(
        PollSender::new(tx).sink_map_err(|_| io::Error::from(ErrorKind::BrokenPipe)),
    );

    // Write data to our interface...
    let data: [u8; 4] = [1, 2, 3, 4];
    let _ = writer.write(&data).await;

    // ... and receive it.
    assert_eq!(data.to_vec(), rx.recv().await.unwrap());

    Ok(())
}
