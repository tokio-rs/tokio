extern crate tokio_buf;
extern crate bytes;
extern crate futures;

use tokio_buf::buf_stream::{BufStream, SizeHint};
use bytes::Buf;
use futures::{Future, Poll};
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

// ===== test `SizeHint` =====

#[test]
fn size_hint() {
    let hint = SizeHint::new();
    assert_eq!(hint.lower(), 0);
    assert!(hint.upper().is_none());

    let mut hint = SizeHint::new();
    hint.set_lower(100);
    assert_eq!(hint.lower(), 100);
    assert!(hint.upper().is_none());

    let mut hint = SizeHint::new();
    hint.set_upper(200);
    assert_eq!(hint.lower(), 0);
    assert_eq!(hint.upper(), Some(200));

    let mut hint = SizeHint::new();
    hint.set_lower(100);
    hint.set_upper(100);
    assert_eq!(hint.lower(), 100);
    assert_eq!(hint.upper(), Some(100));
}

#[test]
#[should_panic]
fn size_hint_lower_bigger_than_upper() {
    let mut hint = SizeHint::new();
    hint.set_upper(100);
    hint.set_lower(200);
}

#[test]
#[should_panic]
fn size_hint_upper_less_than_lower() {
    let mut hint = SizeHint::new();
    hint.set_lower(200);
    hint.set_upper(100);
}

// ===== test `chain()` =====

#[test]
fn chain() {
    // Chain one with one
    //
    let mut bs = one("hello").chain(one("world"));

    assert_buf_eq!(bs.poll_buf(), "hello");
    assert_buf_eq!(bs.poll_buf(), "world");
    assert_none!(bs.poll_buf());

    // Chain multi with multi
    let mut bs = list(&["foo", "bar"])
        .chain(list(&["baz", "bok"]));

    assert_buf_eq!(bs.poll_buf(), "foo");
    assert_buf_eq!(bs.poll_buf(), "bar");
    assert_buf_eq!(bs.poll_buf(), "baz");
    assert_buf_eq!(bs.poll_buf(), "bok");
    assert_none!(bs.poll_buf());

    // Chain includes a not ready call
    //
    let mut bs = new_mock(&[
        Ok(Ready("foo")),
        Ok(NotReady),
        Ok(Ready("bar"))
    ]).chain(one("baz"));

    assert_buf_eq!(bs.poll_buf(), "foo");
    assert_not_ready!(bs.poll_buf());
    assert_buf_eq!(bs.poll_buf(), "bar");
    assert_buf_eq!(bs.poll_buf(), "baz");
    assert_none!(bs.poll_buf());
}

// ===== Test `collect()` =====

#[test]
fn collect_vec() {
    // While unfortunate, this test makes some assumptions on vec's resizing
    // behavior.
    //
    // Collect one
    //
    let bs = one("hello world");

    let vec: Vec<u8> = bs.collect()
        .wait().unwrap();

    assert_eq!(vec, b"hello world");
    assert_eq!(vec.capacity(), 64);

    // Collect one, with size hint
    //
    let mut bs = one("hello world");
    bs.size_hint.set_lower(11);

    let vec: Vec<u8> = bs.collect()
        .wait().unwrap();

    assert_eq!(vec, b"hello world");
    assert_eq!(vec.capacity(), 64);

    // Collect one, with size hint
    //
    let mut bs = one("hello world");
    bs.size_hint.set_lower(10);

    let vec: Vec<u8> = bs.collect()
        .wait().unwrap();

    assert_eq!(vec, b"hello world");
    assert_eq!(vec.capacity(), 64);

    // Collect many
    //
    let bs = list(&["hello", " ", "world", ", one two three"]);

    let vec: Vec<u8> = bs.collect()
        .wait().unwrap();

    assert_eq!(vec, b"hello world, one two three");
}

// ===== Test limit() =====

#[test]
fn limit() {
    // Not limited

    let res = one("hello world")
        .limit(100)
        .collect::<Vec<_>>()
        .wait().unwrap();

    assert_eq!(res, b"hello world");

    let res = list(&["hello", " ", "world"])
        .limit(100)
        .collect::<Vec<_>>()
        .wait().unwrap();

    assert_eq!(res, b"hello world");

    let res = list(&["hello", " ", "world"])
        .limit(11)
        .collect::<Vec<_>>()
        .wait().unwrap();

    assert_eq!(res, b"hello world");

    // Limited

    let res = one("hello world")
        .limit(5)
        .collect::<Vec<_>>()
        .wait();

    assert!(res.is_err());

    let res = one("hello world")
        .limit(10)
        .collect::<Vec<_>>()
        .wait();

    assert!(res.is_err());

    let mut bs = list(&["hello", " ", "world"])
        .limit(9);

    assert_buf_eq!(bs.poll_buf(), "hello");
    assert_buf_eq!(bs.poll_buf(), " ");
    assert!(bs.poll_buf().is_err());

    let mut bs = list(&["hello", " ", "world"]);
    bs.size_hint.set_lower(11);
    let mut bs = bs.limit(9);

    assert!(bs.poll_buf().is_err());
}

// ===== BufStream impelmentations for misc types =====

#[test]
fn str_buf_stream() {
    let mut bs = "hello world".to_string();
    assert_buf_eq!(bs.poll_buf(), "hello world");
    assert!(bs.is_empty());
    assert_none!(bs.poll_buf());

    let mut bs = "hello world";
    assert_buf_eq!(bs.poll_buf(), "hello world");
    assert!(bs.is_empty());
    assert_none!(bs.poll_buf());
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

    Mock {
        polls,
        size_hint: SizeHint::default(),
    }
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

    Mock {
        polls,
        size_hint: SizeHint::default(),
    }
}

#[derive(Debug)]
struct Mock {
    polls: VecDeque<Poll<&'static [u8], ()>>,
    size_hint: SizeHint,
}

#[derive(Debug)]
struct MockBuf {
    data: Cursor<&'static [u8]>,
}

impl BufStream for Mock {
    type Item = MockBuf;
    type Error = ();

    fn poll_buf(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        match self.polls.pop_front() {
            Some(Ok(Ready(value))) => Ok(Ready(Some(MockBuf::new(value)))),
            Some(Ok(NotReady)) => Ok(NotReady),
            Some(Err(e)) => Err(e),
            None => Ok(Ready(None)),
        }
    }

    fn size_hint(&self) -> SizeHint {
        self.size_hint.clone()
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
