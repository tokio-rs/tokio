#![warn(rust_2018_idioms)]

use futures_util::StreamExt;
use tokio_io::AsyncBufReadExt;
use tokio_test::assert_ok;

#[tokio::test]
async fn lines() {
    let rd: &[u8] = b"hello\r\nworld\n\n";
    let mut st = rd.lines();

    let b = assert_ok!(st.next().await.unwrap());
    assert_eq!(b, "hello");
    let b = assert_ok!(st.next().await.unwrap());
    assert_eq!(b, "world");
    let b = assert_ok!(st.next().await.unwrap());
    assert_eq!(b, "");
    assert!(st.next().await.is_none());
}
