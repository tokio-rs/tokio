#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncReadExt, AsyncSeekExt, BufReader};
use tokio_test::assert_ok;

use std::io::{Cursor, SeekFrom};

#[tokio::test]
async fn buf_reader_seek() {
    let mut cursor = Cursor::new(b"hello\nworld\n\n");
    let mut reader = BufReader::new(&mut cursor);

    let mut buf: [u8; 5] = [0; 5];

    let n = assert_ok!(reader.read(&mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf.as_ref(), "hello".as_bytes());

    assert_ok!(reader.seek(SeekFrom::Current(-2)).await);

    let n = assert_ok!(reader.read(&mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf.as_ref(), "lo\nwo".as_bytes());

    assert_ok!(reader.seek(SeekFrom::Start(0)).await);

    let n = assert_ok!(reader.read(&mut buf).await);
    assert_eq!(n, 5);
    assert_eq!(buf.as_ref(), "hello".as_bytes());

    assert_ok!(reader.seek(SeekFrom::End(0)).await);

    let n = assert_ok!(reader.read(&mut buf).await);
    assert_eq!(n, 0);
}
