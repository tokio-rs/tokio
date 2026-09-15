#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::ErrorKind;
use tokio::io::{AsyncBufReadExt, BufReader, Error};
use tokio_test::{assert_ok, io::Builder};

#[tokio::test]
async fn skip_until() {
    let mut rd: &[u8] = b"hello world";

    let n = assert_ok!(rd.skip_until(b' ').await);
    assert_eq!(n, 6);
    let n = assert_ok!(rd.skip_until(b' ').await);
    assert_eq!(n, 5);
    let n = assert_ok!(rd.skip_until(b' ').await);
    assert_eq!(n, 0);
}

#[tokio::test]
async fn skip_until_not_all_ready() {
    let mock = Builder::new()
        .read(b"Hello Wor")
        .read(b"ld#Fizz\xffBuz")
        .read(b"z#1#2")
        .build();

    let mut read = BufReader::new(mock);

    let bytes = read.skip_until(b'#').await.unwrap();
    assert_eq!(bytes, b"Hello World#".len());

    let bytes = read.skip_until(b'#').await.unwrap();
    assert_eq!(bytes, b"Fizz\xffBuzz\n".len());

    let bytes = read.skip_until(b'#').await.unwrap();
    assert_eq!(bytes, 2);

    let bytes = read.skip_until(b'#').await.unwrap();
    assert_eq!(bytes, 1);
}

#[tokio::test]
async fn skip_until_fail() {
    let mock = Builder::new()
        .read(b"Hello \xffWor")
        .read_error(Error::new(ErrorKind::Other, "The world has no end"))
        .build();

    let mut read = BufReader::new(mock);

    let err = read.skip_until(b'#').await.expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::Other);
    assert_eq!(err.to_string(), "The world has no end");
}
