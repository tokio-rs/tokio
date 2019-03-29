extern crate bytes;
extern crate futures;
extern crate tokio_buf;
extern crate tokio_mock_task;

use futures::sync::mpsc;
use futures::Async::*;
use std::io::Cursor;
use tokio_buf::{util, BufStream};
use tokio_mock_task::MockTask;

#[macro_use]
mod support;

type Buf = Cursor<&'static [u8]>;

#[test]
fn empty_stream() {
    let (_, rx) = mpsc::unbounded::<Buf>();
    let mut bs = util::stream(rx);
    assert_none!(bs.poll_buf());
}

#[test]
fn full_stream() {
    let (tx, rx) = mpsc::unbounded();
    let mut bs = util::stream(rx);
    let mut task = MockTask::new();

    tx.unbounded_send(buf(b"one")).unwrap();

    assert_buf_eq!(bs.poll_buf(), "one");
    task.enter(|| assert_not_ready!(bs.poll_buf()));

    tx.unbounded_send(buf(b"two")).unwrap();

    assert!(task.is_notified());
    assert_buf_eq!(bs.poll_buf(), "two");
    task.enter(|| assert_not_ready!(bs.poll_buf()));

    drop(tx);

    assert!(task.is_notified());
    assert_none!(bs.poll_buf());
}

fn buf(data: &'static [u8]) -> Buf {
    Cursor::new(data)
}
