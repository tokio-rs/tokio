#![warn(rust_2018_idioms)]

use std::io;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, Instant};
use tokio_test::io::Builder;

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
async fn write_with_handle() {
    let (mut mock, mut handle) = Builder::new().build_with_handle();
    handle.write(b"hello ");
    handle.write(b"world!");

    mock.write_all(b"hello ").await.expect("write 1");
    mock.write_all(b"world!").await.expect("write 2");
}

#[tokio::test]
async fn read_with_handle() {
    let (mut mock, mut handle) = Builder::new().build_with_handle();
    handle.read(b"hello ");
    handle.read(b"world!");

    let mut buf = vec![0; 6];
    mock.read_exact(&mut buf).await.expect("read 1");
    assert_eq!(&buf[..], b"hello ");
    mock.read_exact(&mut buf).await.expect("read 2");
    assert_eq!(&buf[..], b"world!");
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

#[tokio::test(start_paused = true)]
async fn wait() {
    const FIRST_WAIT: Duration = Duration::from_secs(1);

    let mut mock = Builder::new()
        .wait(FIRST_WAIT)
        .read(b"hello ")
        .read(b"world!")
        .build();

    let mut buf = [0; 256];

    let start = Instant::now(); // record the time the read call takes
                                //
    let n = mock.read(&mut buf).await.expect("read 1");
    assert_eq!(&buf[..n], b"hello ");
    println!("time elapsed after first read {:?}", start.elapsed());

    let n = mock.read(&mut buf).await.expect("read 2");
    assert_eq!(&buf[..n], b"world!");
    println!("time elapsed after second read {:?}", start.elapsed());

    // make sure the .wait() instruction worked
    assert!(
        start.elapsed() >= FIRST_WAIT,
        "consuming the whole mock only took {}ms",
        start.elapsed().as_millis()
    );
}

#[tokio::test(start_paused = true)]
async fn multiple_wait() {
    const FIRST_WAIT: Duration = Duration::from_secs(1);
    const SECOND_WAIT: Duration = Duration::from_secs(1);

    let mut mock = Builder::new()
        .wait(FIRST_WAIT)
        .read(b"hello ")
        .wait(SECOND_WAIT)
        .read(b"world!")
        .build();

    let mut buf = [0; 256];

    let start = Instant::now(); // record the time it takes to consume the mock

    let n = mock.read(&mut buf).await.expect("read 1");
    assert_eq!(&buf[..n], b"hello ");
    println!("time elapsed after first read {:?}", start.elapsed());

    let n = mock.read(&mut buf).await.expect("read 2");
    assert_eq!(&buf[..n], b"world!");
    println!("time elapsed after second read {:?}", start.elapsed());

    // make sure the .wait() instruction worked
    assert!(
        start.elapsed() >= FIRST_WAIT + SECOND_WAIT,
        "consuming the whole mock only took {}ms",
        start.elapsed().as_millis()
    );
}
