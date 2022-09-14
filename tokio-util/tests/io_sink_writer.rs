#![warn(rust_2018_idioms)]

use tokio_util::sync::PollSender;
use tokio_util::io::SinkWriter;
use futures_util::SinkExt;
use tokio::io::AsyncWriteExt;
use std::io::{Error, ErrorKind};

#[tokio::test]
async fn test_sink_writer() -> Result<(), Error> {
   // Construct a channel pair to send data across and wrap a pollable sink.
   // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
   // as its error type.
   let (tx, mut rx) = tokio::sync::mpsc::channel::<u8>(10);
   let mut writer = SinkWriter::new(
       PollSender::new(tx).sink_map_err(|_| Error::from(ErrorKind::Other)),
   );

   // Write data to our interface...
   let data: [u8; 4] = [1, 2, 3, 4];
   writer.write(&data).await?;

   // ... and receive it.
   let mut received = Vec::new();
   for _ in 0..4 {
       received.push(rx.recv().await.unwrap());
   }
   assert_eq!(&data, received.as_slice());
   Ok(())
}
