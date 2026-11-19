#![warn(rust_2018_idioms)]

use std::error::Error as _;
use std::sync::Arc;
use std::{fmt, io};
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
    let error = io::Error::other("cruel");
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
    let error = io::Error::other("cruel");
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
async fn read_errors_with_reused_builder() {
    for (error, expected) in scripted_errors().into_iter().zip(scripted_errors()) {
        let mut builder = Builder::new();
        builder.read_error(error).read(b"after");

        for _ in 0..2 {
            let mut mock = builder.build();
            let mut buf = [0; 5];
            assert_shared_error(&mock.read(&mut buf).await.unwrap_err(), &expected);
            mock.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"after");
        }
    }
}

#[tokio::test]
async fn write_errors_with_cloned_builder() {
    for (error, expected) in scripted_errors().into_iter().zip(scripted_errors()) {
        let mut builder = Builder::new();
        builder.write_error(error).write(b"after");
        let mut cloned = builder.clone();
        let mut mock = builder.build();
        drop(builder);

        assert_shared_error(&mock.write(b"ignored").await.unwrap_err(), &expected);
        mock.write_all(b"after").await.unwrap();

        let mut mock = cloned.build();
        assert_shared_error(&mock.write(b"ignored").await.unwrap_err(), &expected);
        mock.write_all(b"after").await.unwrap();
    }
}

#[tokio::test]
async fn errors_with_sibling_mocks() {
    let mut builder = Builder::new();
    builder
        .read_error(io::ErrorKind::ConnectionReset.into())
        .read(b"r")
        .write_error(io::ErrorKind::BrokenPipe.into())
        .write(b"w");
    let mut mocks = [builder.build(), builder.build()];
    drop(builder);

    for mock in &mut mocks {
        let mut buf = [0];
        assert_eq!(
            mock.read(&mut buf).await.unwrap_err().kind(),
            io::ErrorKind::ConnectionReset
        );
        mock.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"r");
        assert_eq!(
            mock.write(b"ignored").await.unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
        mock.write_all(b"w").await.unwrap();
    }
}

#[tokio::test]
async fn unique_errors_keep_custom_payload() {
    let read_error = io::Error::other(TestError(42));
    let write_error = io::Error::other(TestError(43));
    let payloads = [
        read_error
            .get_ref()
            .unwrap()
            .downcast_ref::<TestError>()
            .unwrap() as *const TestError,
        write_error
            .get_ref()
            .unwrap()
            .downcast_ref::<TestError>()
            .unwrap() as *const TestError,
    ];
    let mut mock = Builder::new()
        .read_error(read_error)
        .write_error(write_error)
        .build();
    let errors = [
        mock.read(&mut [0]).await.unwrap_err(),
        mock.write(b"ignored").await.unwrap_err(),
    ];

    for (index, (mut error, original)) in errors.into_iter().zip(payloads).enumerate() {
        let payload = error
            .get_ref()
            .unwrap()
            .downcast_ref::<TestError>()
            .unwrap();
        assert!(std::ptr::eq(payload, original));
        assert!(error.source().unwrap().is::<TestSource>());
        error
            .get_mut()
            .unwrap()
            .downcast_mut::<TestError>()
            .unwrap()
            .0 += 1;
        let payload = error.into_inner().unwrap().downcast::<TestError>().unwrap();
        assert!(std::ptr::eq(&*payload, original));
        assert_eq!(payload.0, 43 + index);
    }
}

fn scripted_errors() -> [io::Error; 3] {
    [
        io::ErrorKind::ConnectionReset.into(),
        io::Error::from_raw_os_error(13),
        io::Error::new(io::ErrorKind::ConnectionReset, TestError(42)),
    ]
}

fn assert_shared_error(error: &io::Error, expected: &io::Error) {
    assert_eq!(error.kind(), expected.kind());
    assert_eq!(error.raw_os_error(), expected.raw_os_error());
    assert_eq!(error.to_string(), expected.to_string());
    if expected.get_ref().is_some() {
        assert!(error.source().unwrap().is::<TestSource>());
        assert_eq!(error.source().unwrap().to_string(), "test source");
        let payload = error.get_ref().unwrap();
        assert!(!payload.is::<TestError>());
        let original = payload.downcast_ref::<Arc<io::Error>>().unwrap();
        assert_eq!(
            original
                .get_ref()
                .unwrap()
                .downcast_ref::<TestError>()
                .unwrap()
                .0,
            42
        );
    } else {
        assert!(error.get_ref().is_none());
    }
}

#[derive(Debug)]
struct TestError(usize);

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "test error {}", self.0)
    }
}

impl std::error::Error for TestError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&TestSource)
    }
}

#[derive(Debug)]
struct TestSource;

impl fmt::Display for TestSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("test source")
    }
}

impl std::error::Error for TestSource {}

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
