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

#[tokio::test]
async fn interrupted_after_partial_read_is_cooperative() {
    let expected = b"abcd";
    // Preserve the completed prefix across the yield caused by Interrupted retries.
    let reader = {
        let mut builder = tokio_test::io::Builder::new();
        builder.read(&expected[..1]);
        for _ in 0..256 {
            builder.read_error(ErrorKind::Interrupted.into());
        }
        builder.read(&expected[1..]).build()
    };
    let mut read = tokio_test::task::spawn(read_exact_arc(reader, expected.len()));

    tokio_test::assert_pending!(read.poll());
    assert_eq!(&*read.await.unwrap(), expected);
}
