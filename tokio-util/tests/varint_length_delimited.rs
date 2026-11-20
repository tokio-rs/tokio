#![warn(rust_2018_idioms)]

use bytes::{BufMut, Bytes, BytesMut};
use std::io;
use tokio_stream::StreamExt;
use tokio_util::codec::{Decoder, Encoder, FramedRead, VarintLengthDelimitedCodec};

fn encode(codec: &mut VarintLengthDelimitedCodec, data: &[u8]) -> BytesMut {
    let mut dst = BytesMut::new();
    codec
        .encode(data, &mut dst)
        .expect("frame fits in the default maximum");
    dst
}

#[test]
fn encodes_length_as_varint() {
    let mut codec = VarintLengthDelimitedCodec::new();

    assert_eq!(&encode(&mut codec, b"hello")[..], b"\x05hello");
    assert_eq!(&encode(&mut codec, b"")[..], b"\x00");
}

#[test]
fn encodes_multibyte_length_as_varint() {
    let mut codec = VarintLengthDelimitedCodec::new();

    let data = vec![0xab; 300];
    let encoded = encode(&mut codec, &data);

    // 300 == 0b1_0010_1100 -> 0xac 0x02
    assert_eq!(&encoded[..2], &[0xac, 0x02]);
    assert_eq!(encoded.len(), 302);
}

#[test]
fn length_prefix_uses_the_fewest_bytes() {
    let mut codec = VarintLengthDelimitedCodec::new();

    for (len, prefix_len) in [(0, 1), (1, 1), (127, 1), (128, 2), (16_383, 2), (16_384, 3)] {
        let encoded = encode(&mut codec, &vec![0; len]);
        assert_eq!(
            encoded.len() - len,
            prefix_len,
            "unexpected prefix length for a {len} byte frame"
        );
    }
}

#[test]
fn decodes_every_frame_in_a_buffer() {
    let mut codec = VarintLengthDelimitedCodec::new();

    let mut buf = BytesMut::new();
    codec.encode(&b"one"[..], &mut buf).unwrap();
    codec.encode(&b"two"[..], &mut buf).unwrap();
    codec.encode(&[0xab; 300][..], &mut buf).unwrap();

    assert_eq!(codec.decode(&mut buf).unwrap().unwrap().as_ref(), b"one");
    assert_eq!(codec.decode(&mut buf).unwrap().unwrap().as_ref(), b"two");

    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.len(), 300);
    assert_eq!(frame.as_ref(), &[0xab; 300][..]);

    assert!(buf.is_empty());
    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[test]
fn decodes_zero_length_frame() {
    let mut codec = VarintLengthDelimitedCodec::new();
    let mut buf = BytesMut::from(&b"\x00rest"[..]);

    let frame = codec.decode(&mut buf).unwrap().unwrap();

    assert!(frame.is_empty());
    assert_eq!(&buf[..], b"rest");
}

#[test]
fn waits_for_the_whole_payload() {
    let mut codec = VarintLengthDelimitedCodec::new();
    let mut buf = BytesMut::from(&b"\x05hel"[..]);

    assert!(codec.decode(&mut buf).unwrap().is_none());
    // A partial frame must not be consumed.
    assert_eq!(&buf[..], b"\x05hel");

    buf.put_slice(b"lo");

    assert_eq!(codec.decode(&mut buf).unwrap().unwrap().as_ref(), b"hello");
    assert!(buf.is_empty());
}

#[test]
fn waits_for_the_whole_varint() {
    let mut codec = VarintLengthDelimitedCodec::new();
    let mut buf = BytesMut::from(&b"\xac"[..]);

    assert!(codec.decode(&mut buf).unwrap().is_none());
    // A partial varint must not be consumed either.
    assert_eq!(&buf[..], b"\xac");

    buf.put_u8(0x02);
    buf.put_slice(&[0xcd; 300]);

    let frame = codec.decode(&mut buf).unwrap().unwrap();
    assert_eq!(frame.as_ref(), &[0xcd; 300][..]);
}

#[test]
fn rejects_overlong_varint() {
    let mut codec = VarintLengthDelimitedCodec::new();
    let mut buf = BytesMut::from(&[0x80u8; 11][..]);

    let err = codec.decode(&mut buf).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn rejects_varint_that_overflows_u64() {
    let mut codec = VarintLengthDelimitedCodec::new();
    // Nine continuation bytes plus a tenth byte that does not fit in 64 bits.
    let mut buf = BytesMut::from(&[0xffu8; 9][..]);
    buf.put_u8(0x02);

    let err = codec.decode(&mut buf).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn rejects_frame_longer_than_the_limit() {
    let mut codec = VarintLengthDelimitedCodec::new_with_max_frame_length(4);
    let mut buf = BytesMut::from(&b"\x05hello"[..]);

    let err = codec.decode(&mut buf).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn rejects_encoding_frame_longer_than_the_limit() {
    let mut codec = VarintLengthDelimitedCodec::new_with_max_frame_length(4);
    let mut dst = BytesMut::new();

    let err = codec.encode(&b"hello"[..], &mut dst).unwrap_err();

    assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    assert!(dst.is_empty());
}

#[test]
fn decodes_frames_under_the_limit() {
    let mut codec = VarintLengthDelimitedCodec::new_with_max_frame_length(5);
    let mut buf = BytesMut::from(&b"\x05hello\x00"[..]);

    assert_eq!(codec.decode(&mut buf).unwrap().unwrap().as_ref(), b"hello");
    assert!(codec.decode(&mut buf).unwrap().unwrap().is_empty());
}

#[test]
fn decode_eof_reports_truncated_frame() {
    let mut codec = VarintLengthDelimitedCodec::new();

    let mut partial_payload = BytesMut::from(&b"\x05hel"[..]);
    assert!(codec.decode_eof(&mut partial_payload).is_err());

    let mut partial_varint = BytesMut::from(&b"\xac"[..]);
    assert!(codec.decode_eof(&mut partial_varint).is_err());

    let mut complete = BytesMut::from(&b"\x05hello"[..]);
    assert!(codec.decode_eof(&mut complete).unwrap().is_some());

    let mut empty = BytesMut::new();
    assert!(codec.decode_eof(&mut empty).unwrap().is_none());
}

#[test]
fn encodes_bytes_and_bytes_mut() {
    let mut codec = VarintLengthDelimitedCodec::new();

    let mut dst = BytesMut::new();
    codec.encode(Bytes::from_static(b"abc"), &mut dst).unwrap();
    assert_eq!(&dst[..], b"\x03abc");

    let mut dst = BytesMut::new();
    codec.encode(BytesMut::from(&b"de"[..]), &mut dst).unwrap();
    assert_eq!(&dst[..], b"\x02de");
}

#[tokio::test]
async fn framed_read_decodes_varint_frames() {
    let mut framed = FramedRead::new(&b"\x03abc\x05world"[..], VarintLengthDelimitedCodec::new());

    assert_eq!(framed.next().await.unwrap().unwrap().as_ref(), b"abc");
    assert_eq!(framed.next().await.unwrap().unwrap().as_ref(), b"world");
    assert!(framed.next().await.is_none());
}

#[test]
fn absurd_length_prefix_is_pending_not_a_panic() {
    // `usize::MAX` can never be buffered, but announcing it must not overflow
    // the length arithmetic or allocate.
    let mut codec = VarintLengthDelimitedCodec::new_with_max_frame_length(usize::MAX);
    let mut buf = BytesMut::new();
    buf.put_slice(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01]);

    assert!(codec.decode(&mut buf).unwrap().is_none());
}

#[tokio::test]
async fn framed_read_reassembles_large_frames_across_reads() {
    let payload = vec![0x5a; 300_000];

    let mut encoded = BytesMut::new();
    VarintLengthDelimitedCodec::new()
        .encode(&payload[..], &mut encoded)
        .unwrap();

    let mut framed = FramedRead::new(&encoded[..], VarintLengthDelimitedCodec::new());
    let frame = framed.next().await.unwrap().unwrap();

    assert_eq!(frame.as_ref(), &payload[..]);
    assert!(framed.next().await.is_none());
}

/// Independent reference encoder, written with division rather than shifts, so
/// the codec's arithmetic is not compared against itself.
fn reference_varint(mut len: u64) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (len % 128) as u8;
        len /= 128;
        if len == 0 {
            out.push(byte);
            break;
        }
        out.push(byte | 0x80);
    }
    out
}

#[test]
fn roundtrips_against_a_reference_encoder() {
    let mut codec = VarintLengthDelimitedCodec::new_with_max_frame_length(1 << 20);

    let mut lengths: Vec<usize> = (0..300).collect();
    lengths.extend([127, 128, 129, 16_383, 16_384, 16_385, 1 << 20]);

    // Deterministic xorshift so the test is reproducible without a rand dep.
    let mut state = 0x2545_f491_4f6c_dd1du64;
    for _ in 0..200 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        lengths.push((state % 70_000) as usize);
    }

    for len in lengths {
        let payload = vec![0x77; len];

        let mut encoded = BytesMut::new();
        codec.encode(&payload[..], &mut encoded).unwrap();

        let prefix = reference_varint(len as u64);
        assert_eq!(
            &encoded[..prefix.len()],
            &prefix[..],
            "bad prefix for {len}"
        );
        assert_eq!(&encoded[prefix.len()..], payload.as_slice());

        let mut buf = encoded.clone();
        let frame = codec.decode(&mut buf).unwrap().expect("frame is complete");
        assert_eq!(frame.as_ref(), payload.as_slice());
        assert!(buf.is_empty());
    }
}

#[tokio::test]
async fn framed_roundtrip_over_duplex() {
    use futures::SinkExt;
    use tokio::io::duplex;
    use tokio_util::codec::Framed;

    let (client, server) = duplex(1024);

    let writer = tokio::spawn(async move {
        let mut framed = Framed::new(client, VarintLengthDelimitedCodec::new());
        framed.send(Bytes::from_static(b"ping")).await.unwrap();
        framed.send(Bytes::from(vec![0x42; 4_096])).await.unwrap();
    });

    let mut framed = Framed::new(server, VarintLengthDelimitedCodec::new());

    let first = framed.next().await.unwrap().unwrap();
    assert_eq!(first.as_ref(), b"ping");

    let second = framed.next().await.unwrap().unwrap();
    assert_eq!(second.as_ref(), &[0x42; 4_096][..]);

    writer.await.unwrap();
}
