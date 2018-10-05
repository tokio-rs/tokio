extern crate tokio_buf;
extern crate bytes;
extern crate futures;

use tokio_buf::BufStream;
use bytes::Buf;
use futures::Poll;
use futures::Async::*;

use std::collections::VecDeque;
use std::io::Cursor;

macro_rules! assert_buf_eq {
    ($actual:expr, $expect:expr) => {{
        match $actual {
            Ok(Ready(Some(val))) => {
                assert_eq!(val.remaining(), val.bytes().len());
                assert_eq!(val.bytes(), $expect.as_bytes());
            }
            Ok(Ready(None)) => panic!("expected value; BufStream yielded None"),
            Ok(NotReady) => panic!("expected value; BufStream is not ready"),
            Err(e) => panic!("expected value; got error = {:?}", e),
        }
    }};
}

macro_rules! assert_none {
    ($actual:expr) => {
        match $actual {
            Ok(Ready(None)) => {}
            actual => panic!("expected None; actual = {:?}", actual),
        }
    }
}

macro_rules! assert_not_ready {
    ($actual:expr) => {
        match $actual {
            Ok(NotReady) => {}
            actual => panic!("expected NotReady; actual = {:?}", actual),
        }
    }
}

#[test]
fn chain() {
    // Chain one with one
    //
    let mut bs = one("hello").chain(one("world"));

    assert_buf_eq!(bs.poll(), "hello");
    assert_buf_eq!(bs.poll(), "world");
    assert_none!(bs.poll());

    // Chain multi with multi
    let mut bs = list(&["foo", "bar"])
        .chain(list(&["baz", "bok"]));

    assert_buf_eq!(bs.poll(), "foo");
    assert_buf_eq!(bs.poll(), "bar");
    assert_buf_eq!(bs.poll(), "baz");
    assert_buf_eq!(bs.poll(), "bok");
    assert_none!(bs.poll());

    // Chain includes a not ready call
    //
    let mut bs = new_mock(&[
        Ok(Ready("foo")),
        Ok(NotReady),
        Ok(Ready("bar"))
    ]).chain(one("baz"));

    assert_buf_eq!(bs.poll(), "foo");
    assert_not_ready!(bs.poll());
    assert_buf_eq!(bs.poll(), "bar");
    assert_buf_eq!(bs.poll(), "baz");
    assert_none!(bs.poll());
}

// ===== Test utils =====

fn one(buf: &'static str) -> Mock {
    list(&[buf])
}

fn list(bufs: &[&'static str]) -> Mock {
    let mut polls = VecDeque::new();

    for &buf in bufs {
        polls.push_back(Ok(Ready(buf.as_bytes())));
    }

    Mock { polls }
}

fn new_mock(values: &[Poll<&'static str, ()>]) -> Mock {
    let mut polls = VecDeque::new();

    for &v in values {
        polls.push_back(match v {
            Ok(Ready(v)) => Ok(Ready(v.as_bytes())),
            Ok(NotReady) => Ok(NotReady),
            Err(e) => Err(e),
        });
    }

    Mock { polls }
}

#[derive(Debug)]
struct Mock {
    polls: VecDeque<Poll<&'static [u8], ()>>,
}

#[derive(Debug)]
struct MockBuf {
    data: Cursor<&'static [u8]>,
}

impl BufStream for Mock {
    type Item = MockBuf;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.polls.pop_front() {
            Some(Ok(Ready(value))) => Ok(Ready(Some(MockBuf::new(value)))),
            Some(Ok(NotReady)) => Ok(NotReady),
            Some(Err(e)) => Err(e),
            None => Ok(Ready(None)),
        }
    }
}

impl MockBuf {
    fn new(data: &'static [u8]) -> MockBuf {
        MockBuf {
            data: Cursor::new(data),
        }
    }
}

impl Buf for MockBuf {
    fn remaining(&self) -> usize {
        self.data.remaining()
    }

    fn bytes(&self) -> &[u8] {
        self.data.bytes()
    }

    fn advance(&mut self, cnt: usize) {
        self.data.advance(cnt)
    }
}
