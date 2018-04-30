extern crate tokio_io;
extern crate bytes;
extern crate futures;

use tokio_io::AsyncWrite;
use tokio_io::codec::{Encoder, FramedWrite};

use futures::{Sink, Poll};
use bytes::{BytesMut, BufMut, BigEndian};

use std::io::{self, Write};
use std::collections::VecDeque;

macro_rules! mock {
    ($($x:expr,)*) => {{
        let mut v = VecDeque::new();
        v.extend(vec![$($x),*]);
        Mock { calls: v }
    }};
}

struct U32Encoder;

impl Encoder for U32Encoder {
    type Item = u32;
    type Error = io::Error;

    fn encode(&mut self, item: u32, dst: &mut BytesMut) -> io::Result<()> {
        // Reserve space
        dst.reserve(4);
        dst.put_u32_be(item);
        Ok(())
    }
}

#[test]
fn write_multi_frame_in_packet() {
    let mock = mock! {
        Ok(b"\x00\x00\x00\x00\x00\x00\x00\x01\x00\x00\x00\x02".to_vec()),
    };

    let mut framed = FramedWrite::new(mock, U32Encoder);
    assert!(framed.start_send(0).unwrap().is_ready());
    assert!(framed.start_send(1).unwrap().is_ready());
    assert!(framed.start_send(2).unwrap().is_ready());

    // Nothing written yet
    assert_eq!(1, framed.get_ref().calls.len());

    // Flush the writes
    assert!(framed.poll_complete().unwrap().is_ready());

    assert_eq!(0, framed.get_ref().calls.len());
}

#[test]
fn write_hits_backpressure() {
    const ITER: usize = 2 * 1024;

    let mut mock = mock! {
        // Block the `ITER`th write
        Err(io::Error::new(io::ErrorKind::WouldBlock, "not ready")),
        Ok(b"".to_vec()),
    };

    for i in 0..(ITER + 1) {
        let mut b = BytesMut::with_capacity(4);
        b.put_u32_be(i as u32);

        // Append to the end
        match mock.calls.back_mut().unwrap() {
            &mut Ok(ref mut data) => {
                // Write in 2kb chunks
                if data.len() < ITER {
                    data.extend_from_slice(&b[..]);
                    continue;
                }
            }
            _ => unreachable!(),
        }

        // Push a new new chunk
        mock.calls.push_back(Ok(b[..].to_vec()));
    }

    let mut framed = FramedWrite::new(mock, U32Encoder);

    for i in 0..ITER {
        assert!(framed.start_send(i as u32).unwrap().is_ready());
    }

    // This should reject
    assert!(!framed.start_send(ITER as u32).unwrap().is_ready());

    // This should succeed and start flushing the buffer.
    assert!(framed.start_send(ITER as u32).unwrap().is_ready());

    // Flush the rest of the buffer
    assert!(framed.poll_complete().unwrap().is_ready());

    // Ensure the mock is empty
    assert_eq!(0, framed.get_ref().calls.len());
}

// ===== Mock ======

struct Mock {
    calls: VecDeque<io::Result<Vec<u8>>>,
}

impl Write for Mock {
    fn write(&mut self, src: &[u8]) -> io::Result<usize> {
        match self.calls.pop_front() {
            Some(Ok(data)) => {
                assert!(src.len() >= data.len());
                assert_eq!(&data[..], &src[..data.len()]);
                Ok(data.len())
            }
            Some(Err(e)) => Err(e),
            None => panic!("unexpected write; {:?}", src),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncWrite for Mock {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(().into())
    }
}
