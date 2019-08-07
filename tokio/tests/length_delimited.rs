// #![deny(warnings, rust_2018_idioms)]

use tokio::codec::*;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::prelude::*;
use tokio_test::assert_ready;
use tokio_test::task::MockTask;

use futures_util::pin_mut;
use bytes::{BufMut, Bytes, BytesMut};
use std::collections::VecDeque;
use std::pin::Pin;
use std::io;
use std::task::{Context, Poll};
use std::task::Poll::*;

macro_rules! mock {
    ($($x:expr,)*) => {{
        let mut v = VecDeque::new();
        v.extend(vec![$($x),*]);
        Mock { calls: v }
    }};
}

macro_rules! assert_next_eq {
    ($io:ident, $expect:expr) => {{
        MockTask::new().enter(|cx| {
            let res = assert_ready!($io.as_mut().poll_next(cx));
            match res {
                Some(Ok(v)) => assert_eq!(v, $expect.as_ref()),
                Some(Err(e)) => panic!("error = {:?}", e),
                None => panic!("none"),
            }
        });
    }}
}

macro_rules! assert_pending {
    ($io:ident) => {{
        MockTask::new().enter(|cx| {
            match $io.as_mut().poll_next(cx) {
                Ready(Some(Ok(v))) => panic!("value = {:?}", v),
                Ready(Some(Err(e))) => panic!("error = {:?}", e),
                Ready(None) => panic!("done"),
                Pending => {}
            }
        });
    }}
}

macro_rules! assert_err {
    ($io:ident) => {{
        MockTask::new().enter(|cx| {
            match $io.as_mut().poll_next(cx) {
                Ready(Some(Ok(v))) => panic!("value = {:?}", v),
                Ready(Some(Err(_))) => {}
                Ready(None) => panic!("done"),
                Pending => panic!("pending"),
            }
        });
    }}
}

macro_rules! assert_done {
    ($io:ident) => {{
        MockTask::new().enter(|cx| {
            let res = assert_ready!($io.as_mut().poll_next(cx));
            match res {
                Some(Ok(v)) => panic!("value = {:?}", v),
                Some(Err(e)) => panic!("error = {:?}", e),
                None => {}
            }
        });
    }}
}

#[test]
fn read_empty_io_yields_nothing() {
    let io = Box::pin(FramedRead::new(mock!(), LengthDelimitedCodec::new()));
    pin_mut!(io);

    assert_done!(io);
}

#[test]
fn read_single_frame_one_packet() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00\x00\x09abcdefghi"),
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_done!(io);
}

#[test]
fn read_single_frame_one_packet_little_endian() {
    let io = length_delimited::Builder::new()
        .little_endian()
        .new_read(mock! {
            data(b"\x09\x00\x00\x00abcdefghi"),
        });
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_done!(io);
}

#[test]
fn read_single_frame_one_packet_native_endian() {
    let d = if cfg!(target_endian = "big") {
        b"\x00\x00\x00\x09abcdefghi"
    } else {
        b"\x09\x00\x00\x00abcdefghi"
    };
    let io = length_delimited::Builder::new()
        .native_endian()
        .new_read(mock! {
            data(d),
        });
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_done!(io);
}

#[test]
fn read_single_multi_frame_one_packet() {
    let mut d: Vec<u8> = vec![];
    d.extend_from_slice(b"\x00\x00\x00\x09abcdefghi");
    d.extend_from_slice(b"\x00\x00\x00\x03123");
    d.extend_from_slice(b"\x00\x00\x00\x0bhello world");

    let io = FramedRead::new(
        mock! {
            data(&d),
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_next_eq!(io, b"123");
    assert_next_eq!(io, b"hello world");
    assert_done!(io);
}

#[test]
fn read_single_frame_multi_packet() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00"),
            data(b"\x00\x09abc"),
            data(b"defghi"),
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_done!(io);
}

#[test]
fn read_multi_frame_multi_packet() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00"),
            data(b"\x00\x09abc"),
            data(b"defghi"),
            data(b"\x00\x00\x00\x0312"),
            data(b"3\x00\x00\x00\x0bhello world"),
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_next_eq!(io, b"123");
    assert_next_eq!(io, b"hello world");
    assert_done!(io);
}

#[test]
fn read_single_frame_multi_packet_wait() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00"),
            Pending,
            data(b"\x00\x09abc"),
            Pending,
            data(b"defghi"),
            Pending,
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_pending!(io);
    assert_pending!(io);
    assert_next_eq!(io, b"abcdefghi");
    assert_pending!(io);
    assert_done!(io);
}

#[test]
fn read_multi_frame_multi_packet_wait() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00"),
            Pending,
            data(b"\x00\x09abc"),
            Pending,
            data(b"defghi"),
            Pending,
            data(b"\x00\x00\x00\x0312"),
            Pending,
            data(b"3\x00\x00\x00\x0bhello world"),
            Pending,
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_pending!(io);
    assert_pending!(io);
    assert_next_eq!(io, b"abcdefghi");
    assert_pending!(io);
    assert_pending!(io);
    assert_next_eq!(io, b"123");
    assert_next_eq!(io, b"hello world");
    assert_pending!(io);
    assert_done!(io);
}

#[test]
fn read_incomplete_head() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00"),
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_err!(io);
}

#[test]
fn read_incomplete_head_multi() {
    let io = FramedRead::new(
        mock! {
            Pending,
            data(b"\x00"),
            Pending,
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_pending!(io);
    assert_pending!(io);
    assert_err!(io);
}

#[test]
fn read_incomplete_payload() {
    let io = FramedRead::new(
        mock! {
            data(b"\x00\x00\x00\x09ab"),
            Pending,
            data(b"cd"),
            Pending,
        },
        LengthDelimitedCodec::new(),
    );
    pin_mut!(io);

    assert_pending!(io);
    assert_pending!(io);
    assert_err!(io);
}

#[test]
fn read_max_frame_len() {
    let io = length_delimited::Builder::new()
        .max_frame_length(5)
        .new_read(mock! {
            data(b"\x00\x00\x00\x09abcdefghi"),
        });
    pin_mut!(io);

    assert_err!(io);
}

#[test]
fn read_update_max_frame_len_at_rest() {
    let io = length_delimited::Builder::new().new_read(mock! {
        data(b"\x00\x00\x00\x09abcdefghi"),
        data(b"\x00\x00\x00\x09abcdefghi"),
    });
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    io.decoder_mut().set_max_frame_length(5);
    assert_err!(io);
}

#[test]
fn read_update_max_frame_len_in_flight() {
    let io = length_delimited::Builder::new().new_read(mock! {
        data(b"\x00\x00\x00\x09abcd"),
        Pending,
        data(b"efghi"),
        data(b"\x00\x00\x00\x09abcdefghi"),
    });
    pin_mut!(io);

    assert_pending!(io);
    io.decoder_mut().set_max_frame_length(5);
    assert_next_eq!(io, b"abcdefghi");
    assert_err!(io);
}

#[test]
fn read_one_byte_length_field() {
    let io = length_delimited::Builder::new()
        .length_field_length(1)
        .new_read(mock! {
            data(b"\x09abcdefghi"),
        });
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_done!(io);
}

#[test]
fn read_header_offset() {
    let io = length_delimited::Builder::new()
        .length_field_length(2)
        .length_field_offset(4)
        .new_read(mock! {
            data(b"zzzz\x00\x09abcdefghi"),
        });
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_done!(io);
}

#[test]
fn read_single_multi_frame_one_packet_skip_none_adjusted() {
    let mut d: Vec<u8> = vec![];
    d.extend_from_slice(b"xx\x00\x09abcdefghi");
    d.extend_from_slice(b"yy\x00\x03123");
    d.extend_from_slice(b"zz\x00\x0bhello world");

    let io = length_delimited::Builder::new()
        .length_field_length(2)
        .length_field_offset(2)
        .num_skip(0)
        .length_adjustment(4)
        .new_read(mock! {
            data(&d),
        });
    pin_mut!(io);

    assert_next_eq!(io, b"xx\x00\x09abcdefghi");
    assert_next_eq!(io, b"yy\x00\x03123");
    assert_next_eq!(io, b"zz\x00\x0bhello world");
    assert_done!(io);
}

#[test]
fn read_single_multi_frame_one_packet_length_includes_head() {
    let mut d: Vec<u8> = vec![];
    d.extend_from_slice(b"\x00\x0babcdefghi");
    d.extend_from_slice(b"\x00\x05123");
    d.extend_from_slice(b"\x00\x0dhello world");

    let io = length_delimited::Builder::new()
        .length_field_length(2)
        .length_adjustment(-2)
        .new_read(mock! {
            data(&d),
        });
    pin_mut!(io);

    assert_next_eq!(io, b"abcdefghi");
    assert_next_eq!(io, b"123");
    assert_next_eq!(io, b"hello world");
    assert_done!(io);
}

/*
#[test]
fn write_single_frame_length_adjusted() {
    let mut io = length_delimited::Builder::new()
        .length_adjustment(-2)
        .new_write(mock! {
            Ok(b"\x00\x00\x00\x0b"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(Flush),
        });
    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_nothing_yields_nothing() {
    let mut io = FramedWrite::new(mock!(), LengthDelimitedCodec::new());
    assert!(io.poll_complete().unwrap().is_ready());
}

#[test]
fn write_single_frame_one_packet() {
    let mut io = FramedWrite::new(
        mock! {
            Ok(b"\x00\x00\x00\x09"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(Flush),
        },
        LengthDelimitedCodec::new(),
    );

    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_single_multi_frame_one_packet() {
    let mut io = FramedWrite::new(
        mock! {
            Ok(b"\x00\x00\x00\x09"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(b"\x00\x00\x00\x03"[..].into()),
            Ok(b"123"[..].into()),
            Ok(b"\x00\x00\x00\x0b"[..].into()),
            Ok(b"hello world"[..].into()),
            Ok(Flush),
        },
        LengthDelimitedCodec::new(),
    );

    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(io.start_send(Bytes::from("123")).unwrap().is_ready());
    assert!(io
        .start_send(Bytes::from("hello world"))
        .unwrap()
        .is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_single_multi_frame_multi_packet() {
    let mut io = FramedWrite::new(
        mock! {
            Ok(b"\x00\x00\x00\x09"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(Flush),
            Ok(b"\x00\x00\x00\x03"[..].into()),
            Ok(b"123"[..].into()),
            Ok(Flush),
            Ok(b"\x00\x00\x00\x0b"[..].into()),
            Ok(b"hello world"[..].into()),
            Ok(Flush),
        },
        LengthDelimitedCodec::new(),
    );

    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.start_send(Bytes::from("123")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io
        .start_send(Bytes::from("hello world"))
        .unwrap()
        .is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_single_frame_would_block() {
    let mut io = FramedWrite::new(
        mock! {
            Err(would_block()),
            Ok(b"\x00\x00"[..].into()),
            Err(would_block()),
            Ok(b"\x00\x09"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(Flush),
        },
        LengthDelimitedCodec::new(),
    );

    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(!io.poll_complete().unwrap().is_ready());
    assert!(!io.poll_complete().unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());

    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_single_frame_little_endian() {
    let mut io = length_delimited::Builder::new()
        .little_endian()
        .new_write(mock! {
            Ok(b"\x09\x00\x00\x00"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(Flush),
        });

    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_single_frame_with_short_length_field() {
    let mut io = length_delimited::Builder::new()
        .length_field_length(1)
        .new_write(mock! {
            Ok(b"\x09"[..].into()),
            Ok(b"abcdefghi"[..].into()),
            Ok(Flush),
        });

    assert!(io.start_send(Bytes::from("abcdefghi")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_max_frame_len() {
    let mut io = length_delimited::Builder::new()
        .max_frame_length(5)
        .new_write(mock! {});

    assert_eq!(
        io.start_send(Bytes::from("abcdef")).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_update_max_frame_len_at_rest() {
    let mut io = length_delimited::Builder::new().new_write(mock! {
        Ok(b"\x00\x00\x00\x06"[..].into()),
        Ok(b"abcdef"[..].into()),
        Ok(Flush),
    });

    assert!(io.start_send(Bytes::from("abcdef")).unwrap().is_ready());
    assert!(io.poll_complete().unwrap().is_ready());
    io.encoder_mut().set_max_frame_length(5);
    assert_eq!(
        io.start_send(Bytes::from("abcdef")).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_update_max_frame_len_in_flight() {
    let mut io = length_delimited::Builder::new().new_write(mock! {
        Ok(b"\x00\x00\x00\x06"[..].into()),
        Ok(b"ab"[..].into()),
        Err(would_block()),
        Ok(b"cdef"[..].into()),
        Ok(Flush),
    });

    assert!(io.start_send(Bytes::from("abcdef")).unwrap().is_ready());
    assert!(!io.poll_complete().unwrap().is_ready());
    io.encoder_mut().set_max_frame_length(5);
    assert!(io.poll_complete().unwrap().is_ready());
    assert_eq!(
        io.start_send(Bytes::from("abcdef")).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn write_zero() {
    let mut io = length_delimited::Builder::new().new_write(mock! {});

    assert!(io.start_send(Bytes::from("abcdef")).unwrap().is_ready());
    assert_eq!(
        io.poll_complete().unwrap_err().kind(),
        io::ErrorKind::WriteZero
    );
    assert!(io.get_ref().calls.is_empty());
}

#[test]
fn encode_overflow() {
    // Test reproducing tokio-rs/tokio#681.
    let mut codec = length_delimited::Builder::new().new_codec();
    let mut buf = BytesMut::with_capacity(1024);

    // Put some data into the buffer without resizing it to hold more.
    let some_as = std::iter::repeat(b'a').take(1024).collect::<Vec<_>>();
    buf.put_slice(&some_as[..]);

    // Trying to encode the length header should resize the buffer if it won't fit.
    codec.encode(Bytes::from("hello"), &mut buf).unwrap();
}
*/

// ===== Test utils =====

struct Mock {
    calls: VecDeque<Poll<io::Result<Op>>>,
}

enum Op {
    Data(Vec<u8>),
    Flush,
}

use self::Op::*;

impl AsyncRead for Mock {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        dst: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.calls.pop_front() {
            Some(Ready(Ok(Op::Data(data)))) => {
                debug_assert!(dst.len() >= data.len());
                dst[..data.len()].copy_from_slice(&data[..]);
                Ready(Ok(data.len()))
            }
            Some(Ready(Ok(_))) => panic!(),
            Some(Ready(Err(e))) => Ready(Err(e)),
            Some(Pending) => Pending,
            None => Ready(Ok(0)),
        }
    }
}

impl AsyncWrite for Mock {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        src: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        match self.calls.pop_front() {
            Some(Ready(Ok(Op::Data(data)))) => {
                let len = data.len();
                assert!(src.len() >= len, "expect={:?}; actual={:?}", data, src);
                assert_eq!(&data[..], &src[..len]);
                Ready(Ok(len))
            }
            Some(Ready(Ok(_))) => panic!(),
            Some(Ready(Err(e))) => Ready(Err(e)),
            Some(Pending) => Pending,
            None => Ready(Ok(0)),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        match self.calls.pop_front() {
            Some(Ready(Ok(Op::Flush))) => Ready(Ok(())),
            Some(Ready(Ok(_))) => panic!(),
            Some(Ready(Err(e))) => Ready(Err(e)),
            Some(Pending) => Pending,
            None => Ready(Ok(())),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Ready(Ok(()))
    }
}

impl<'a> From<&'a [u8]> for Op {
    fn from(src: &'a [u8]) -> Op {
        Op::Data(src.into())
    }
}

impl From<Vec<u8>> for Op {
    fn from(src: Vec<u8>) -> Op {
        Op::Data(src)
    }
}

fn data(bytes: &[u8]) -> Poll<io::Result<Op>> {
    Ready(Ok(bytes.into()))
}
