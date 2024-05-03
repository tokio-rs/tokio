#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::ErrorKind;
use tokio::io::{AsyncBufReadExt, BufReader, Error};
use tokio_test::{assert_ok, io::Builder};

#[tokio::test]
async fn read_until_slice_single_byte() {
    let mut buf = vec![];
    let mut rd: &[u8] = b"hello world";

    let n = assert_ok!(rd.read_until_slice(b" ", &mut buf).await);
    assert_eq!(n, 6);
    assert_eq!(buf, b"hello ");
    buf.clear();
    let n = assert_ok!(rd.read_until_slice(b" ", &mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf, b"world");
    buf.clear();
    let n = assert_ok!(rd.read_until_slice(b" ", &mut buf).await);
    assert_eq!(n, 0);
    assert_eq!(buf, []);
}

#[tokio::test]
async fn read_until_slice_two_bytes() {
    let mut buf = vec![];
    let mut rd: &[u8] = b"hello  world";

    let n = assert_ok!(rd.read_until_slice(b"  ", &mut buf).await);
    assert_eq!(n, 7);
    assert_eq!(buf, b"hello  ");
    buf.clear();
    let n = assert_ok!(rd.read_until_slice(b"  ", &mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf, b"world");
    buf.clear();
    let n = assert_ok!(rd.read_until_slice(b"  ", &mut buf).await);
    assert_eq!(n, 0);
    assert_eq!(buf, []);
}

#[tokio::test]
async fn read_until_slice_not_all_ready_single_byte() {
    let mock = Builder::new()
        .read(b"Hello Wor")
        .read(b"ld#Fizz\xffBuz")
        .read(b"z#1#2")
        .build();

    let mut read = BufReader::new(mock);

    let mut chunk = b"We say ".to_vec();
    let bytes = read.read_until_slice(b"#", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Hello World#".len());
    assert_eq!(chunk, b"We say Hello World#");

    chunk = b"I solve ".to_vec();
    let bytes = read.read_until_slice(b"#", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Fizz\xffBuzz#".len());
    assert_eq!(chunk, b"I solve Fizz\xffBuzz#");

    chunk.clear();
    let bytes = read.read_until_slice(b"#", &mut chunk).await.unwrap();
    assert_eq!(bytes, 2);
    assert_eq!(chunk, b"1#");

    chunk.clear();
    let bytes = read.read_until_slice(b"#", &mut chunk).await.unwrap();
    assert_eq!(bytes, 1);
    assert_eq!(chunk, b"2");
}

#[tokio::test]
async fn read_until_slice_not_all_ready_mutliple_bytes() {
    let mock = Builder::new()
        .read(b"Hello Wor")
        .read(b"ld! #Fizz\xffBuz")
        .read(b"z###1###2")
        .build();

    let mut read = BufReader::new(mock);

    let mut chunk = b"We say ".to_vec();
    let bytes = read.read_until_slice(b"! #", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Hello World! #".len());
    assert_eq!(chunk, b"We say Hello World! #");

    chunk = b"I solve ".to_vec();
    let bytes = read.read_until_slice(b"###", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Fizz\xffBuzz###".len());
    assert_eq!(chunk, b"I solve Fizz\xffBuzz###");

    chunk.clear();
    let bytes = read.read_until_slice(b"###", &mut chunk).await.unwrap();
    assert_eq!(bytes, 4);
    assert_eq!(chunk, b"1###");

    chunk.clear();
    let bytes = read.read_until_slice(b"###", &mut chunk).await.unwrap();
    assert_eq!(bytes, 1);
    assert_eq!(chunk, b"2");
}

#[tokio::test]
async fn read_until_slice_not_all_ready_mid_delimiter() {
    let mock = Builder::new()
        .read(b"Hello World! ")
        .read(b" Welcome!")
        .read(b" ")
        .read(b" --")
        .build();

    let mut read = BufReader::new(mock);

    let mut chunk = b"We say ".to_vec();
    let bytes = read.read_until_slice(b"!  ", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Hello World!  ".len());
    assert_eq!(chunk, b"We say Hello World!  ");

    chunk = b"You are ".to_vec();
    let bytes = read.read_until_slice(b"!  ", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Welcome!  ".len());
    assert_eq!(chunk, b"You are Welcome!  ");

    chunk.clear();
    let bytes = read.read_until_slice(b"!  ", &mut chunk).await.unwrap();
    assert_eq!(bytes, 2);
    assert_eq!(chunk, b"--");
}

#[tokio::test]
async fn read_until_slice_fail_single_byte() {
    let mock = Builder::new()
        .read(b"Hello \xffWor")
        .read_error(Error::new(ErrorKind::Other, "The world has no end"))
        .build();

    let mut read = BufReader::new(mock);

    let mut chunk = b"Foo".to_vec();
    let err = read
        .read_until_slice(b"#", &mut chunk)
        .await
        .expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::Other);
    assert_eq!(err.to_string(), "The world has no end");
    assert_eq!(chunk, b"FooHello \xffWor");
}

#[tokio::test]
async fn read_until_slice_fail_two_bytes() {
    let mock = Builder::new()
        .read(b"Hello \xffWor")
        .read_error(Error::new(ErrorKind::Other, "The world has no end"))
        .build();

    let mut read = BufReader::new(mock);

    let mut chunk = b"Foo".to_vec();
    let err = read
        .read_until_slice(b"\xff\xff", &mut chunk)
        .await
        .expect_err("Should fail");
    assert_eq!(err.kind(), ErrorKind::Other);
    assert_eq!(err.to_string(), "The world has no end");
    assert_eq!(chunk, b"FooHello \xffWor");
}

#[tokio::test]
async fn read_until_slice_small_buffer() {
    let mock = Builder::new().read(b"Value\r").read(b"\nOther").build();

    let mut read = BufReader::with_capacity(2, mock);

    let mut chunk = b"Foo".to_vec();
    let bytes = read.read_until_slice(b"\r\n", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Value\r\n".len());
    assert_eq!(chunk, b"FooValue\r\n");

    chunk.clear();
    let bytes = read.read_until_slice(b"\r\n", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Other".len());
    assert_eq!(chunk, b"Other");
}

#[tokio::test]
async fn read_until_slice_split_delimiter() {
    let mock = Builder::new()
        .read(b"Some\r")
        .read(b"Other\n")
        .read(b"Value")
        .build();

    let mut read = BufReader::new(mock);

    let mut chunk = b"Foo".to_vec();
    let bytes = read.read_until_slice(b"\r\n", &mut chunk).await.unwrap();
    assert_eq!(bytes, b"Some\rOther\nValue".len());
    assert_eq!(chunk, b"FooSome\rOther\nValue");
}
