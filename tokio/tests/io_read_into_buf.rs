#![warn(rust_2018_idioms)]

use std::pin::Pin;

use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio_test::assert_ok;

#[tokio::test]
async fn read_into_buf() {
    let rd: &[u8] = b"12345";
    let mut rd = BufReader::with_capacity(2, rd);

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 2);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"12");

    Pin::new(&mut rd).consume(1);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"2");

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 0);
    Pin::new(&mut rd).consume(1);

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 2);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"34");

    Pin::new(&mut rd).consume(2);

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 1);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"5");
}

#[tokio::test]
async fn read_into_buf_read_again() {
    let rd = b"1".chain(&b"23"[..]);
    let mut rd = BufReader::with_capacity(2, rd);

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 1);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"1");

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 1);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"12");

    Pin::new(&mut rd).consume(1);

    assert_eq!(Pin::new(&mut rd).get_buf(), b"2");

    Pin::new(&mut rd).consume(1);

    assert_eq!(Pin::new(&mut rd).get_buf(), b"");

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 1);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"3");

    Pin::new(&mut rd).consume(1);

    let read = assert_ok!(rd.read_into_buf().await);
    assert_eq!(read, 0);
    assert_eq!(Pin::new(&mut rd).get_buf(), b"");
}
