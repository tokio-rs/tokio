extern crate tokio_io;
extern crate bytes;

use bytes::{BytesMut, Bytes, BufMut};
use tokio_io::codec::{BytesCodec, LinesCodec, Decoder, Encoder};

#[test]
fn bytes_decoder() {
    let mut codec = BytesCodec::new();
    let buf = &mut BytesMut::new();
    buf.put_slice(b"abc");
    assert_eq!("abc", codec.decode(buf).unwrap().unwrap());
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode(buf).unwrap());
    buf.put_slice(b"a");
    assert_eq!("a", codec.decode(buf).unwrap().unwrap());
}

#[test]
fn bytes_encoder() {
    let mut codec = BytesCodec::new();

    // Default capacity of BytesMut
    #[cfg(target_pointer_width = "64")]
    const INLINE_CAP: usize = 4 * 8 - 1;
    #[cfg(target_pointer_width = "32")]
    const INLINE_CAP: usize = 4 * 4 - 1;

    let mut buf = BytesMut::new();
    codec.encode(Bytes::from_static(&[0; INLINE_CAP + 1]), &mut buf).unwrap();

    // Default capacity of Framed Read
    const INITIAL_CAPACITY: usize = 8 * 1024;

    let mut buf = BytesMut::with_capacity(INITIAL_CAPACITY);
    codec.encode(Bytes::from_static(&[0; INITIAL_CAPACITY + 1]), &mut buf).unwrap();
}

#[test]
fn lines_decoder() {
    let mut codec = LinesCodec::new();
    let buf = &mut BytesMut::new();
    buf.reserve(200);
    buf.put("line 1\nline 2\r\nline 3\n\r\n\r");
    assert_eq!("line 1", codec.decode(buf).unwrap().unwrap());
    assert_eq!("line 2", codec.decode(buf).unwrap().unwrap());
    assert_eq!("line 3", codec.decode(buf).unwrap().unwrap());
    assert_eq!("", codec.decode(buf).unwrap().unwrap());
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode_eof(buf).unwrap());
    buf.put("k");
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!("\rk", codec.decode_eof(buf).unwrap().unwrap());
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode_eof(buf).unwrap());
}

#[test]
fn lines_encoder() {
    let mut codec = BytesCodec::new();

    // Default capacity of BytesMut
    #[cfg(target_pointer_width = "64")]
    const INLINE_CAP: usize = 4 * 8 - 1;
    #[cfg(target_pointer_width = "32")]
    const INLINE_CAP: usize = 4 * 4 - 1;

    let mut buf = BytesMut::new();
    codec.encode(Bytes::from_static(&[b'a'; INLINE_CAP + 1]), &mut buf).unwrap();

    // Default capacity of Framed Read
    const INITIAL_CAPACITY: usize = 8 * 1024;

    let mut buf = BytesMut::with_capacity(INITIAL_CAPACITY);
    codec.encode(Bytes::from_static(&[b'a'; INITIAL_CAPACITY + 1]), &mut buf).unwrap();
}
