use std::io;
use tokio::io::AsyncReadExt;
use tokio_test::io::Builder;

#[tokio::test]
async fn to_string_does_not_truncate_on_utf8_error() {
    let data = vec![0xff, 0xff, 0xff];

    let mut s = "abc".to_string();

    match AsyncReadExt::read_to_string(&mut data.as_slice(), &mut s).await {
        Ok(len) => panic!("Should fail: {} bytes.", len),
        Err(err) if err.to_string() == "stream did not contain valid UTF-8" => {}
        Err(err) => panic!("Fail: {}.", err),
    }

    assert_eq!(s, "abc");
}

#[tokio::test]
async fn to_string_does_not_truncate_on_io_error() {
    let mut mock = Builder::new()
        .read(b"def")
        .read_error(io::Error::new(io::ErrorKind::Other, "whoops"))
        .build();
    let mut s = "abc".to_string();

    match AsyncReadExt::read_to_string(&mut mock, &mut s).await {
        Ok(len) => panic!("Should fail: {} bytes.", len),
        Err(err) if err.to_string() == "whoops" => {}
        Err(err) => panic!("Fail: {}.", err),
    }

    assert_eq!(s, "abc");
}

#[tokio::test]
async fn to_string_appends() {
    let data = b"def".to_vec();

    let mut s = "abc".to_string();

    let len = AsyncReadExt::read_to_string(&mut data.as_slice(), &mut s)
        .await
        .unwrap();

    assert_eq!(len, 3);
    assert_eq!(s, "abcdef");
}
