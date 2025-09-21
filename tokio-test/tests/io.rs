#![warn(rust_2018_idioms)]

use futures_test::task::{noop_context, panic_waker};
use futures_util::pin_mut;
use std::future::Future;
use std::io;
use std::task::{Context, Poll};
use tokio::io::AsyncWrite;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, Instant};
use tokio_test::io::Builder;
use tokio_test::{assert_pending, assert_ready};

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
            assert_eq!("cruel", format!("{error}"));
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
            assert_eq!("cruel", format!("{error}"));
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

// No matter which usecase, it doesn't make sense for a read
// hang forever. However, currently, if there is no sequenced read
// action, it will hang forever.
//
// Since we want be aware of the fixing of this bug,
// no matter intentionally or unintentionally,
// we add this test to catch the behavior change.
//
// It looks like fixing it is not hard, but not sure the downstream
// impact, which might be a breaking change due to the
// `Mock::inner::read_wait` field, so we keep it as is for now.
//
// TODO: fix this bug
#[test]
fn should_hang_forever_on_read_but_no_sequenced_read_action() {
    let mut mock = Builder::new()
        .write_error(io::Error::new(io::ErrorKind::Other, "cruel"))
        .build();

    let mut buf = [0; 1];
    let read_exact_fut = mock.read(&mut buf);
    pin_mut!(read_exact_fut);
    assert_pending!(read_exact_fut.poll(&mut Context::from_waker(&panic_waker())));
}

// The `Mock` is expected to always panic if there is an unconsumed error action,
// rather than silently ignoring it. However,
// currently it only panics on unconsumed read/write actions,
// not on error actions. Fixing this requires a breaking change.
//
// This test verifies that it does not panic yet,
// to prevent accidentally introducing the breaking change prematurely.
//
// TODO: fix this bug in the next major release
#[test]
fn do_not_panic_unconsumed_error() {
    let _mock = Builder::new()
        .read_error(io::Error::new(io::ErrorKind::Other, "cruel"))
        .build();
}

// The `Mock` must never panic, even if cloned multiple times.
// However, at present, cloning the builder under certain
// conditions causes a panic.
//
// Fixing this would require making `Mock` non-`Clone`,
// which is a breaking change.
//
// Since we want be aware of the fixing of this bug,
// no matter intentionally or unintentionally,
// we add this test to catch the behavior change.
//
// TODO: fix this bug in the next major release
#[tokio::test]
#[should_panic = "There are no other references.: Custom { kind: Other, error: \"cruel\" }"]
async fn should_panic_if_clone_the_builder_with_error_action() {
    let mut builder = Builder::new();
    builder.write_error(io::Error::new(io::ErrorKind::Other, "cruel"));
    let mut builder2 = builder.clone();

    let mut mock = builder.build();
    let _mock2 = builder2.build();

    // this write_all will panic due to unwrapping the error from `Arc`
    mock.write_all(b"hello").await.unwrap();
    unreachable!();
}

#[tokio::test]
async fn should_not_hang_forever_on_zero_length_write() {
    let mock = Builder::new().write(b"write").build();
    pin_mut!(mock);
    match mock.as_mut().poll_write(&mut noop_context(), &[0u8; 0]) {
        // drain the remaining write action to avoid panic at drop of the `mock`
        Poll::Ready(Ok(0)) => mock.write_all(b"write").await.unwrap(),
        Poll::Ready(Ok(n)) => panic!("expected to write 0 bytes, wrote {n} bytes instead"),
        Poll::Ready(Err(e)) => panic!("expected to write 0 bytes, got error {e} instead"),
        Poll::Pending => panic!("expected to write 0 bytes immediately, but pending instead"),
    }
}

#[tokio::test]
async fn shutdown() {
    let mut mock = Builder::new().shutdown().build();
    mock.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_error() {
    let error = io::Error::new(io::ErrorKind::Other, "cruel");
    let mut mock = Builder::new().shutdown_error(error).build();
    let err = mock.shutdown().await.unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::Other);
    assert_eq!("cruel", format!("{err}"));
}

#[tokio::test(start_paused = true)]
async fn shutdown_wait() {
    const WAIT: Duration = Duration::from_secs(1);

    let mock = Builder::new().wait(WAIT).shutdown().build();
    pin_mut!(mock);

    assert_pending!(mock.as_mut().poll_shutdown(&mut noop_context()));

    tokio::time::advance(WAIT).await;
    let _ = assert_ready!(mock.as_mut().poll_shutdown(&mut noop_context()));
}

#[test]
#[should_panic = "AsyncWrite::poll_shutdown was not called. (1 actions remain)"]
fn should_panic_on_leftover_shutdown_action() {
    let _mock = Builder::new().shutdown().build();
}

#[test]
fn should_not_panic_on_leftover_shutdown_error_action() {
    let _mock = Builder::new()
        .shutdown_error(io::Error::new(io::ErrorKind::Other, "cruel"))
        .build();
}
