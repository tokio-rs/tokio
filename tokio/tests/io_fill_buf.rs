#![warn(rust_2018_idioms)]

use std::pin::Pin;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};
use tokio_test::assert_ok;

#[tokio::test]
async fn fill_buf() {
    let rd: &[u8] = b"12345";
    let mut rd = BufReader::with_capacity(2, rd);

    let buf = assert_ok!(rd.fill_buf().await);
    assert_eq!(buf, b"12");

    let buf = assert_ok!(rd.fill_buf().await);
    assert_eq!(buf, b"12");

    Pin::new(&mut rd).consume(1);

    let buf = assert_ok!(rd.fill_buf().await);
    assert_eq!(buf, b"2");

    Pin::new(&mut rd).consume(1);

    let buf = assert_ok!(rd.fill_buf().await);
    assert_eq!(buf, b"34");

    Pin::new(&mut rd).consume(2);

    let buf = assert_ok!(rd.fill_buf().await);
    assert_eq!(buf, b"5");

    Pin::new(&mut rd).consume(1);

    let buf = assert_ok!(rd.fill_buf().await);
    assert_eq!(buf, b"");
}
