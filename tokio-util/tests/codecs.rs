#![warn(rust_2018_idioms)]

use tokio_util::codec::{BytesCodec, Decoder, Encoder, LinesCodec};

use bytes::{BufMut, Bytes, BytesMut};

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
    codec
        .encode(Bytes::from_static(&[0; INLINE_CAP + 1]), &mut buf)
        .unwrap();

    // Default capacity of Framed Read
    const INITIAL_CAPACITY: usize = 8 * 1024;

    let mut buf = BytesMut::with_capacity(INITIAL_CAPACITY);
    codec
        .encode(Bytes::from_static(&[0; INITIAL_CAPACITY + 1]), &mut buf)
        .unwrap();
}

#[test]
fn lines_decoder() {
    let mut codec = LinesCodec::new();
    let buf = &mut BytesMut::new();
    buf.reserve(200);
    buf.put_slice(b"line 1\nline 2\r\nline 3\n\r\n\r");
    assert_eq!("line 1", codec.decode(buf).unwrap().unwrap());
    assert_eq!("line 2", codec.decode(buf).unwrap().unwrap());
    assert_eq!("line 3", codec.decode(buf).unwrap().unwrap());
    assert_eq!("", codec.decode(buf).unwrap().unwrap());
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode_eof(buf).unwrap());
    buf.put_slice(b"k");
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!("\rk", codec.decode_eof(buf).unwrap().unwrap());
    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode_eof(buf).unwrap());
}

#[test]
fn lines_decoder_max_length() {
    const MAX_LENGTH: usize = 6;

    let mut codec = LinesCodec::new_with_max_length(MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put_slice(b"line 1 is too long\nline 2\nline 3\r\nline 4\n\r\n\r");

    assert!(codec.decode(buf).is_err());

    let line = codec.decode(buf).unwrap().unwrap();
    assert!(
        line.len() <= MAX_LENGTH,
        "{:?}.len() <= {:?}",
        line,
        MAX_LENGTH
    );
    assert_eq!("line 2", line);

    assert!(codec.decode(buf).is_err());

    let line = codec.decode(buf).unwrap().unwrap();
    assert!(
        line.len() <= MAX_LENGTH,
        "{:?}.len() <= {:?}",
        line,
        MAX_LENGTH
    );
    assert_eq!("line 4", line);

    let line = codec.decode(buf).unwrap().unwrap();
    assert!(
        line.len() <= MAX_LENGTH,
        "{:?}.len() <= {:?}",
        line,
        MAX_LENGTH
    );
    assert_eq!("", line);

    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode_eof(buf).unwrap());
    buf.put_slice(b"k");
    assert_eq!(None, codec.decode(buf).unwrap());

    let line = codec.decode_eof(buf).unwrap().unwrap();
    assert!(
        line.len() <= MAX_LENGTH,
        "{:?}.len() <= {:?}",
        line,
        MAX_LENGTH
    );
    assert_eq!("\rk", line);

    assert_eq!(None, codec.decode(buf).unwrap());
    assert_eq!(None, codec.decode_eof(buf).unwrap());

    // Line that's one character too long. This could cause an out of bounds
    // error if we peek at the next characters using slice indexing.
    buf.put_slice(b"aaabbbc");
    assert!(codec.decode(buf).is_err());
}

#[test]
fn lines_decoder_max_length_underrun() {
    const MAX_LENGTH: usize = 6;

    let mut codec = LinesCodec::new_with_max_length(MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put_slice(b"line ");
    assert_eq!(None, codec.decode(buf).unwrap());
    buf.put_slice(b"too l");
    assert!(codec.decode(buf).is_err());
    buf.put_slice(b"ong\n");
    assert_eq!(None, codec.decode(buf).unwrap());

    buf.put_slice(b"line 2");
    assert_eq!(None, codec.decode(buf).unwrap());
    buf.put_slice(b"\n");
    assert_eq!("line 2", codec.decode(buf).unwrap().unwrap());
}

#[test]
fn lines_decoder_max_length_bursts() {
    const MAX_LENGTH: usize = 10;

    let mut codec = LinesCodec::new_with_max_length(MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put_slice(b"line ");
    assert_eq!(None, codec.decode(buf).unwrap());
    buf.put_slice(b"too l");
    assert_eq!(None, codec.decode(buf).unwrap());
    buf.put_slice(b"ong\n");
    assert!(codec.decode(buf).is_err());
}

#[test]
fn lines_decoder_max_length_big_burst() {
    const MAX_LENGTH: usize = 10;

    let mut codec = LinesCodec::new_with_max_length(MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put_slice(b"line ");
    assert_eq!(None, codec.decode(buf).unwrap());
    buf.put_slice(b"too long!\n");
    assert!(codec.decode(buf).is_err());
}

#[test]
fn lines_decoder_max_length_newline_between_decodes() {
    const MAX_LENGTH: usize = 5;

    let mut codec = LinesCodec::new_with_max_length(MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put_slice(b"hello");
    assert_eq!(None, codec.decode(buf).unwrap());

    buf.put_slice(b"\nworld");
    assert_eq!("hello", codec.decode(buf).unwrap().unwrap());
}

// Regression test for [infinite loop bug](https://github.com/tokio-rs/tokio/issues/1483)
#[test]
fn lines_decoder_discard_repeat() {
    const MAX_LENGTH: usize = 1;

    let mut codec = LinesCodec::new_with_max_length(MAX_LENGTH);
    let buf = &mut BytesMut::new();

    buf.reserve(200);
    buf.put_slice(b"aa");
    assert!(codec.decode(buf).is_err());
    buf.put_slice(b"a");
    assert!(codec.decode(buf).is_err());
}

#[test]
fn lines_encoder() {
    let mut codec = LinesCodec::new();
    let mut buf = BytesMut::new();

    codec.encode(String::from("line 1"), &mut buf).unwrap();
    assert_eq!("line 1\n", buf);

    codec.encode(String::from("line 2"), &mut buf).unwrap();
    assert_eq!("line 1\nline 2\n", buf);
}
