#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::ErrorKind;
use tokio_util::io::read_exact_arc;

#[tokio::test]
async fn retry_on_io_interrupted() {
    let reader = tokio_test::io::Builder::new()
        .read_error(ErrorKind::Interrupted.into())
        .read(b"ab")
        .read_error(ErrorKind::Interrupted.into())
        .read_error(ErrorKind::Interrupted.into())
        .read(b"cd")
        .build();
    assert_eq!(&*read_exact_arc(reader, 4).await.unwrap(), b"abcd");
}
