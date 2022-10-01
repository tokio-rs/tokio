#![warn(rust_2018_idioms)]

use futures_util::SinkExt;
use std::fmt::{self, Debug, Display};
use std::io::{self, Error, ErrorKind};
use tokio::io::AsyncWriteExt;
use tokio_util::io::SinkWriter;
use tokio_util::sync::{PollSendError, PollSender};

#[derive(Debug)]
struct PollSendErrorCoupler<T>(PollSendError<T>);

impl<T: Debug> Display for PollSendErrorCoupler<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self, f)
    }
}

impl<T: Debug> std::error::Error for PollSendErrorCoupler<T> {}

impl<T> From<PollSendError<T>> for PollSendErrorCoupler<T> {
    fn from(e: PollSendError<T>) -> Self {
        PollSendErrorCoupler(e)
    }
}
impl<T> Into<io::Error> for PollSendErrorCoupler<T> {
    fn into(self) -> io::Error {
        io::Error::from(ErrorKind::BrokenPipe)
    }
    // add code here
}

#[tokio::test]
async fn test_sink_writer() -> Result<(), Error> {
    // Construct a channel pair to send data across and wrap a pollable sink.
    // Note that the sink must mimic a writable object, e.g. have `std::io::Error`
    // as its error type.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<&[u8]>(1);
    let writer = SinkWriter::new(
        PollSender::new(tx).sink_map_err(|_| io::Error::from(ErrorKind::BrokenPipe)),
    );

    // Write data to our interface...

    let data: [u8; 4] = [1, 2, 3, 4];
    //let _ = writer.write(&data);

    // ... and receive it.
    assert_eq!(&data, rx.recv().await.unwrap());

    Ok(())
}
