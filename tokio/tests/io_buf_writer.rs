use tokio::io::vec::AsyncVectoredWriteExt;
use tokio::io::BufWriter;
use tokio::prelude::*;

use tokio_test::assert_ok;

use std::io::IoSlice;

mod support {
    pub(crate) mod io_vec;
}
use support::io_vec::IoBufs;

#[tokio::test]
async fn write_vectored_empty() {
    let mut w = BufWriter::new(Vec::new());
    let n = assert_ok!(w.write_vectored(&[]).await);
    assert_eq!(n, 0);

    let io_vec = [IoSlice::new(&[]); 3];
    let n = assert_ok!(w.write_vectored(&io_vec).await);
    assert_eq!(n, 0);

    assert_ok!(w.flush().await);
    assert!(w.get_ref().is_empty());
}

#[tokio::test]
async fn write_vectored_basic() {
    let msg = b"foo bar baz";
    let bufs = [
        IoSlice::new(&msg[0..4]),
        IoSlice::new(&msg[4..8]),
        IoSlice::new(&msg[8..]),
    ];
    let mut w = BufWriter::new(Vec::new());
    let n = assert_ok!(w.write_vectored(&bufs).await);
    assert_eq!(n, msg.len());
    assert!(w.buffer() == &msg[..]);
    assert_ok!(w.flush().await);
    assert_eq!(w.get_ref(), msg);

    let mut bufs = [
        IoSlice::new(&msg[0..4]),
        IoSlice::new(&msg[4..8]),
        IoSlice::new(&msg[8..]),
    ];
    let io_vec = IoBufs::new(&mut bufs);
    let mut w = BufWriter::with_capacity(8, Vec::new());
    let n = assert_ok!(w.write_vectored(&io_vec).await);
    assert_eq!(n, 8);
    assert!(w.buffer() == &msg[..8]);
    let io_vec = io_vec.advance(n);
    let n = assert_ok!(w.write_vectored(&io_vec).await);
    assert_eq!(n, 3);
    assert!(w.get_ref().as_slice() == &msg[..8]);
    assert!(w.buffer() == &msg[8..]);
}

struct VectoredWriteHarness {
    writer: BufWriter<Vec<u8>>,
    buf_capacity: usize,
}

impl VectoredWriteHarness {
    fn new(buf_capacity: usize) -> Self {
        VectoredWriteHarness {
            writer: BufWriter::with_capacity(buf_capacity, Vec::new()),
            buf_capacity,
        }
    }

    async fn write_all<'a, 'b>(&mut self, mut io_vec: IoBufs<'a, 'b>) -> usize {
        let mut total_written = 0;
        while !io_vec.is_empty() {
            let n = assert_ok!(self.writer.write_vectored(&io_vec).await);
            assert!(n != 0);
            assert!(self.writer.buffer().len() <= self.buf_capacity);
            total_written += n;
            io_vec = io_vec.advance(n);
        }
        total_written
    }

    async fn flush(&mut self) -> &[u8] {
        assert_ok!(self.writer.flush().await);
        self.writer.get_ref()
    }
}

#[tokio::test]
async fn write_vectored_odd() {
    let msg = b"foo bar baz";
    let mut bufs = [
        IoSlice::new(&msg[0..4]),
        IoSlice::new(&[]),
        IoSlice::new(&msg[4..9]),
        IoSlice::new(&msg[9..]),
    ];
    let mut h = VectoredWriteHarness::new(8);
    let bytes_written = h.write_all(IoBufs::new(&mut bufs)).await;
    assert_eq!(bytes_written, msg.len());
    assert_eq!(h.flush().await, msg);
}

#[tokio::test]
async fn write_vectored_large() {
    let msg = b"foo bar baz";
    let mut bufs = [
        IoSlice::new(&[]),
        IoSlice::new(&msg[..9]),
        IoSlice::new(&msg[9..]),
    ];
    let mut h = VectoredWriteHarness::new(8);
    let bytes_written = h.write_all(IoBufs::new(&mut bufs)).await;
    assert_eq!(bytes_written, msg.len());
    assert_eq!(h.flush().await, msg);
}
