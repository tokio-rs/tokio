#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::ErrorKind;

#[tokio::test]
async fn retry_on_io_interrupted() {
    let mut reader = tokio_test::io::Builder::new()
        .read_error(ErrorKind::Interrupted.into())
        .read(b"ab")
        .read_error(ErrorKind::Interrupted.into())
        .read(b"cd")
        .build();
    let mut writer = tokio_test::io::Builder::new()
        .write_error(ErrorKind::Interrupted.into())
        .write(b"a")
        .write_error(ErrorKind::Interrupted.into())
        .write(b"bcd")
        .build();
    let count = tokio::io::copy_buf(
        &mut tokio::io::BufReader::with_capacity(2, &mut reader),
        &mut writer,
    )
    .await;
    assert_eq!(count.unwrap(), 4);
}
