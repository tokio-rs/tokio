#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::io::{AsyncSeekExt, AsyncWriteExt, BufWriter};
use tokio_test::assert_ok;

use std::io::{Cursor, SeekFrom};

#[tokio::test]
async fn buf_writer_seek() {
    let mut cursor = Cursor::new(Vec::new());
    let mut writer = BufWriter::new(&mut cursor);

    let n = assert_ok!(writer.write("hello".as_bytes()).await);
    assert_eq!(n, 5);

    assert_ok!(writer.seek(SeekFrom::Current(-2)).await);

    let n = assert_ok!(writer.write("world".as_bytes()).await);
    assert_eq!(n, 5);

    assert_ok!(writer.seek(SeekFrom::Start(0)).await);

    let n = assert_ok!(writer.write("hello ".as_bytes()).await);
    assert_eq!(n, 6);

    assert_ok!(writer.seek(SeekFrom::End(-2)).await);

    let n = assert_ok!(writer.write("world!".as_bytes()).await);
    assert_eq!(n, 6);

    assert_ok!(writer.flush().await);
    assert_eq!(cursor.get_ref().as_slice(), "hello world!".as_bytes());
}
