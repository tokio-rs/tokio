#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

macro_rules! ready {
    ($e:expr $(,)?) => {
        match $e {
            std::task::Poll::Ready(t) => t,
            std::task::Poll::Pending => return std::task::Poll::Pending,
        }
    };
}

#[macro_export]
macro_rules! cfg_fs {
    ($($item:item)*) => { $($item)* }
}

#[macro_export]
macro_rules! cfg_io_std {
    ($($item:item)*) => { $($item)* }
}

use futures::future;

// Load source
#[allow(warnings)]
#[path = "../src/fs/file.rs"]
mod file;
use file::File;

#[allow(warnings)]
#[path = "../src/io/blocking.rs"]
mod blocking;

// Load mocked types
mod support {
    pub(crate) mod mock_file;
    pub(crate) mod mock_pool;
}
pub(crate) use support::mock_pool as pool;

// Place them where the source expects them
pub(crate) mod io {
    pub(crate) use tokio::io::*;

    pub(crate) use crate::blocking;

    pub(crate) mod sys {
        pub(crate) use crate::support::mock_pool::{run, Blocking};
    }
}
pub(crate) mod fs {
    pub(crate) mod sys {
        pub(crate) use crate::support::mock_file::File;
        pub(crate) use crate::support::mock_pool::{run, Blocking};
    }

    pub(crate) use crate::support::mock_pool::asyncify;
}
use fs::sys;

use tokio::prelude::*;
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok, task};

use std::io::SeekFrom;

const HELLO: &[u8] = b"hello world...";
const FOO: &[u8] = b"foo bar baz...";

#[test]
fn open_read() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO);

    let mut file = File::from_std(file);

    let mut buf = [0; 1024];
    let mut t = task::spawn(file.read(&mut buf));

    assert_eq!(0, pool::len());
    assert_pending!(t.poll());

    assert_eq!(1, mock.remaining());
    assert_eq!(1, pool::len());

    pool::run_one();

    assert_eq!(0, mock.remaining());
    assert!(t.is_woken());

    let n = assert_ready_ok!(t.poll());
    assert_eq!(n, HELLO.len());
    assert_eq!(&buf[..n], HELLO);
}

#[test]
fn read_twice_before_dispatch() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO);

    let mut file = File::from_std(file);

    let mut buf = [0; 1024];
    let mut t = task::spawn(file.read(&mut buf));

    assert_pending!(t.poll());
    assert_pending!(t.poll());

    assert_eq!(pool::len(), 1);
    pool::run_one();

    assert!(t.is_woken());

    let n = assert_ready_ok!(t.poll());
    assert_eq!(&buf[..n], HELLO);
}

#[test]
fn read_with_smaller_buf() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO);

    let mut file = File::from_std(file);

    {
        let mut buf = [0; 32];
        let mut t = task::spawn(file.read(&mut buf));
        assert_pending!(t.poll());
    }

    pool::run_one();

    {
        let mut buf = [0; 4];
        let mut t = task::spawn(file.read(&mut buf));
        let n = assert_ready_ok!(t.poll());
        assert_eq!(n, 4);
        assert_eq!(&buf[..], &HELLO[..n]);
    }

    // Calling again immediately succeeds with the rest of the buffer
    let mut buf = [0; 32];
    let mut t = task::spawn(file.read(&mut buf));
    let n = assert_ready_ok!(t.poll());
    assert_eq!(n, 10);
    assert_eq!(&buf[..n], &HELLO[4..]);

    assert_eq!(0, pool::len());
}

#[test]
fn read_with_bigger_buf() {
    let (mock, file) = sys::File::mock();
    mock.read(&HELLO[..4]).read(&HELLO[4..]);

    let mut file = File::from_std(file);

    {
        let mut buf = [0; 4];
        let mut t = task::spawn(file.read(&mut buf));
        assert_pending!(t.poll());
    }

    pool::run_one();

    {
        let mut buf = [0; 32];
        let mut t = task::spawn(file.read(&mut buf));
        let n = assert_ready_ok!(t.poll());
        assert_eq!(n, 4);
        assert_eq!(&buf[..n], &HELLO[..n]);
    }

    // Calling again immediately succeeds with the rest of the buffer
    let mut buf = [0; 32];
    let mut t = task::spawn(file.read(&mut buf));

    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());

    let n = assert_ready_ok!(t.poll());
    assert_eq!(n, 10);
    assert_eq!(&buf[..n], &HELLO[4..]);

    assert_eq!(0, pool::len());
}

#[test]
fn read_err_then_read_success() {
    let (mock, file) = sys::File::mock();
    mock.read_err().read(&HELLO);

    let mut file = File::from_std(file);

    {
        let mut buf = [0; 32];
        let mut t = task::spawn(file.read(&mut buf));
        assert_pending!(t.poll());

        pool::run_one();

        assert_ready_err!(t.poll());
    }

    {
        let mut buf = [0; 32];
        let mut t = task::spawn(file.read(&mut buf));
        assert_pending!(t.poll());

        pool::run_one();

        let n = assert_ready_ok!(t.poll());

        assert_eq!(n, HELLO.len());
        assert_eq!(&buf[..n], HELLO);
    }
}

#[test]
fn open_write() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));

    assert_eq!(0, pool::len());
    assert_ready_ok!(t.poll());

    assert_eq!(1, mock.remaining());
    assert_eq!(1, pool::len());

    pool::run_one();

    assert_eq!(0, mock.remaining());
    assert!(!t.is_woken());

    let mut t = task::spawn(file.flush());
    assert_ready_ok!(t.poll());
}

#[test]
fn flush_while_idle() {
    let (_mock, file) = sys::File::mock();

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.flush());
    assert_ready_ok!(t.poll());
}

#[test]
fn read_with_buffer_larger_than_max() {
    // Chunks
    let a = 16 * 1024;
    let b = a * 2;
    let c = a * 3;
    let d = a * 4;

    assert_eq!(d / 1024, 64);

    let mut data = vec![];
    for i in 0..(d - 1) {
        data.push((i % 151) as u8);
    }

    let (mock, file) = sys::File::mock();
    mock.read(&data[0..a])
        .read(&data[a..b])
        .read(&data[b..c])
        .read(&data[c..]);

    let mut file = File::from_std(file);

    let mut actual = vec![0; d];
    let mut pos = 0;

    while pos < data.len() {
        let mut t = task::spawn(file.read(&mut actual[pos..]));

        assert_pending!(t.poll());
        pool::run_one();
        assert!(t.is_woken());

        let n = assert_ready_ok!(t.poll());
        assert!(n <= a);

        pos += n;
    }

    assert_eq!(mock.remaining(), 0);
    assert_eq!(data, &actual[..data.len()]);
}

#[test]
fn write_with_buffer_larger_than_max() {
    // Chunks
    let a = 16 * 1024;
    let b = a * 2;
    let c = a * 3;
    let d = a * 4;

    assert_eq!(d / 1024, 64);

    let mut data = vec![];
    for i in 0..(d - 1) {
        data.push((i % 151) as u8);
    }

    let (mock, file) = sys::File::mock();
    mock.write(&data[0..a])
        .write(&data[a..b])
        .write(&data[b..c])
        .write(&data[c..]);

    let mut file = File::from_std(file);

    let mut rem = &data[..];

    let mut first = true;

    while !rem.is_empty() {
        let mut t = task::spawn(file.write(rem));

        if !first {
            assert_pending!(t.poll());
            pool::run_one();
            assert!(t.is_woken());
        }

        first = false;

        let n = assert_ready_ok!(t.poll());

        rem = &rem[n..];
    }

    pool::run_one();

    assert_eq!(mock.remaining(), 0);
}

#[test]
fn write_twice_before_dispatch() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO).write(FOO);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.write(FOO));
    assert_pending!(t.poll());

    assert_eq!(pool::len(), 1);
    pool::run_one();

    assert!(t.is_woken());

    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.flush());
    assert_pending!(t.poll());

    assert_eq!(pool::len(), 1);
    pool::run_one();

    assert!(t.is_woken());
    assert_ready_ok!(t.poll());
}

#[test]
fn incomplete_read_followed_by_write() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO)
        .seek_current_ok(-(HELLO.len() as i64), 0)
        .write(FOO);

    let mut file = File::from_std(file);

    let mut buf = [0; 32];

    let mut t = task::spawn(file.read(&mut buf));
    assert_pending!(t.poll());

    pool::run_one();

    let mut t = task::spawn(file.write(FOO));
    assert_ready_ok!(t.poll());

    assert_eq!(pool::len(), 1);
    pool::run_one();

    let mut t = task::spawn(file.flush());
    assert_ready_ok!(t.poll());
}

#[test]
fn incomplete_partial_read_followed_by_write() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO).seek_current_ok(-10, 0).write(FOO);

    let mut file = File::from_std(file);

    let mut buf = [0; 32];
    let mut t = task::spawn(file.read(&mut buf));
    assert_pending!(t.poll());

    pool::run_one();

    let mut buf = [0; 4];
    let mut t = task::spawn(file.read(&mut buf));
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.write(FOO));
    assert_ready_ok!(t.poll());

    assert_eq!(pool::len(), 1);
    pool::run_one();

    let mut t = task::spawn(file.flush());
    assert_ready_ok!(t.poll());
}

#[test]
fn incomplete_read_followed_by_flush() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO)
        .seek_current_ok(-(HELLO.len() as i64), 0)
        .write(FOO);

    let mut file = File::from_std(file);

    let mut buf = [0; 32];

    let mut t = task::spawn(file.read(&mut buf));
    assert_pending!(t.poll());

    pool::run_one();

    let mut t = task::spawn(file.flush());
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.write(FOO));
    assert_ready_ok!(t.poll());

    pool::run_one();
}

#[test]
fn incomplete_flush_followed_by_write() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO).write(FOO);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    let n = assert_ready_ok!(t.poll());
    assert_eq!(n, HELLO.len());

    let mut t = task::spawn(file.flush());
    assert_pending!(t.poll());

    // TODO: Move under write
    pool::run_one();

    let mut t = task::spawn(file.write(FOO));
    assert_ready_ok!(t.poll());

    pool::run_one();

    let mut t = task::spawn(file.flush());
    assert_ready_ok!(t.poll());
}

#[test]
fn read_err() {
    let (mock, file) = sys::File::mock();
    mock.read_err();

    let mut file = File::from_std(file);

    let mut buf = [0; 1024];
    let mut t = task::spawn(file.read(&mut buf));

    assert_pending!(t.poll());

    pool::run_one();
    assert!(t.is_woken());

    assert_ready_err!(t.poll());
}

#[test]
fn write_write_err() {
    let (mock, file) = sys::File::mock();
    mock.write_err();

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    pool::run_one();

    let mut t = task::spawn(file.write(FOO));
    assert_ready_err!(t.poll());
}

#[test]
fn write_read_write_err() {
    let (mock, file) = sys::File::mock();
    mock.write_err().read(HELLO);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    pool::run_one();

    let mut buf = [0; 1024];
    let mut t = task::spawn(file.read(&mut buf));

    assert_pending!(t.poll());

    pool::run_one();

    let mut t = task::spawn(file.write(FOO));
    assert_ready_err!(t.poll());
}

#[test]
fn write_read_flush_err() {
    let (mock, file) = sys::File::mock();
    mock.write_err().read(HELLO);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    pool::run_one();

    let mut buf = [0; 1024];
    let mut t = task::spawn(file.read(&mut buf));

    assert_pending!(t.poll());

    pool::run_one();

    let mut t = task::spawn(file.flush());
    assert_ready_err!(t.poll());
}

#[test]
fn write_seek_write_err() {
    let (mock, file) = sys::File::mock();
    mock.write_err().seek_start_ok(0);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    pool::run_one();

    {
        let mut t = task::spawn(file.seek(SeekFrom::Start(0)));
        assert_pending!(t.poll());
    }

    pool::run_one();

    let mut t = task::spawn(file.write(FOO));
    assert_ready_err!(t.poll());
}

#[test]
fn write_seek_flush_err() {
    let (mock, file) = sys::File::mock();
    mock.write_err().seek_start_ok(0);

    let mut file = File::from_std(file);

    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    pool::run_one();

    {
        let mut t = task::spawn(file.seek(SeekFrom::Start(0)));
        assert_pending!(t.poll());
    }

    pool::run_one();

    let mut t = task::spawn(file.flush());
    assert_ready_err!(t.poll());
}

#[test]
fn sync_all_ordered_after_write() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO).sync_all();

    let mut file = File::from_std(file);
    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.sync_all());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_ready_ok!(t.poll());
}

#[test]
fn sync_all_err_ordered_after_write() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO).sync_all_err();

    let mut file = File::from_std(file);
    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.sync_all());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_ready_err!(t.poll());
}

#[test]
fn sync_data_ordered_after_write() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO).sync_data();

    let mut file = File::from_std(file);
    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.sync_data());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_ready_ok!(t.poll());
}

#[test]
fn sync_data_err_ordered_after_write() {
    let (mock, file) = sys::File::mock();
    mock.write(HELLO).sync_data_err();

    let mut file = File::from_std(file);
    let mut t = task::spawn(file.write(HELLO));
    assert_ready_ok!(t.poll());

    let mut t = task::spawn(file.sync_data());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_pending!(t.poll());

    assert_eq!(1, pool::len());
    pool::run_one();

    assert!(t.is_woken());
    assert_ready_err!(t.poll());
}

#[test]
fn open_set_len_ok() {
    let (mock, file) = sys::File::mock();
    mock.set_len(123);

    let mut file = File::from_std(file);
    let mut t = task::spawn(file.set_len(123));

    assert_pending!(t.poll());
    assert_eq!(1, mock.remaining());

    pool::run_one();
    assert_eq!(0, mock.remaining());

    assert!(t.is_woken());
    assert_ready_ok!(t.poll());
}

#[test]
fn open_set_len_err() {
    let (mock, file) = sys::File::mock();
    mock.set_len_err(123);

    let mut file = File::from_std(file);
    let mut t = task::spawn(file.set_len(123));

    assert_pending!(t.poll());
    assert_eq!(1, mock.remaining());

    pool::run_one();
    assert_eq!(0, mock.remaining());

    assert!(t.is_woken());
    assert_ready_err!(t.poll());
}

#[test]
fn partial_read_set_len_ok() {
    let (mock, file) = sys::File::mock();
    mock.read(HELLO)
        .seek_current_ok(-14, 0)
        .set_len(123)
        .read(FOO);

    let mut buf = [0; 32];
    let mut file = File::from_std(file);

    {
        let mut t = task::spawn(file.read(&mut buf));
        assert_pending!(t.poll());
    }

    pool::run_one();

    {
        let mut t = task::spawn(file.set_len(123));

        assert_pending!(t.poll());
        pool::run_one();
        assert_ready_ok!(t.poll());
    }

    let mut t = task::spawn(file.read(&mut buf));
    assert_pending!(t.poll());
    pool::run_one();
    let n = assert_ready_ok!(t.poll());

    assert_eq!(n, FOO.len());
    assert_eq!(&buf[..n], FOO);
}
