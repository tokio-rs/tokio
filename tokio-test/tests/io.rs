#![warn(rust_2018_idioms)]

use std::future::Future;
use std::io;
use futures_core::{Stream, TryStream};


use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use tokio_test::io::Builder;
use tokio_test::io_stream::StreamBuilder;

#[tokio::test]
async fn read() {
    let mut mock = Builder::new().read(b"hello ").read(b"world!").build();

    let mut buf = [0; 256];

    let n = mock.read(&mut buf).await.expect("read 1");
    assert_eq!(&buf[..n], b"hello ");

    let n = mock.read(&mut buf).await.expect("read 2");
    assert_eq!(&buf[..n], b"world!");
}

#[tokio::test]
async fn read_error() {
    let error = io::Error::new(io::ErrorKind::Other, "cruel");
    let mut mock = Builder::new()
        .read(b"hello ")
        .read_error(error)
        .read(b"world!")
        .build();
    let mut buf = [0; 256];

    let n = mock.read(&mut buf).await.expect("read 1");
    assert_eq!(&buf[..n], b"hello ");

    match mock.read(&mut buf).await {
        Err(error) => {
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert_eq!("cruel", format!("{}", error));
        }
        Ok(_) => panic!("error not received"),
    }

    let n = mock.read(&mut buf).await.expect("read 1");
    assert_eq!(&buf[..n], b"world!");
}

#[tokio::test]
async fn write() {
    let mut mock = Builder::new().write(b"hello ").write(b"world!").build();

    mock.write_all(b"hello ").await.expect("write 1");
    mock.write_all(b"world!").await.expect("write 2");
}

#[tokio::test]
async fn write_error() {
    let error = io::Error::new(io::ErrorKind::Other, "cruel");
    let mut mock = Builder::new()
        .write(b"hello ")
        .write_error(error)
        .write(b"world!")
        .build();
    mock.write_all(b"hello ").await.expect("write 1");

    match mock.write_all(b"whoa").await {
        Err(error) => {
            assert_eq!(error.kind(), io::ErrorKind::Other);
            assert_eq!("cruel", format!("{}", error));
        }
        Ok(_) => panic!("error not received"),
    }

    mock.write_all(b"world!").await.expect("write 2");
}

#[tokio::test]
#[should_panic]
async fn mock_panics_read_data_left() {
    use tokio_test::io::Builder;
    Builder::new().read(b"read").build();
}

#[tokio::test]
#[should_panic]
async fn mock_panics_write_data_left() {
    use tokio_test::io::Builder;
    Builder::new().write(b"write").build();
}

#[tokio::test]
async fn stream_read(){
    let mut mock = StreamBuilder::new()
        .read_stream(String::from("hello "))
        .build();
    let txt =  String::from("hello ");
    let res =  mock.next().await;
    assert_eq!(res.unwrap(), txt);
    assert_eq!(res, Some(txt));
}

#[tokio::test]
async fn stream_write(){
    let mut mock = StreamBuilder::new()
        .write_stream(String::from("hello "))
        .build();
    let txt =  String::from("hello ");
    let res =  mock.next().await;
    assert_eq!(res.unwrap(), txt);
    assert_eq!(res, Some(txt));
}