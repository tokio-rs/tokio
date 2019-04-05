#![allow(unused)]

extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use bytes::Buf;
use futures::Async::*;
use futures::Poll;
use tokio_buf::{BufStream, SizeHint};

use std::collections::VecDeque;
use std::io::Cursor;

macro_rules! assert_buf_eq {
    ($actual:expr, $expect:expr) => {{
        use bytes::Buf;
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
    };
}

macro_rules! assert_not_ready {
    ($actual:expr) => {
        match $actual {
            Ok(NotReady) => {}
            actual => panic!("expected NotReady; actual = {:?}", actual),
        }
    };
}

// ===== Test utils =====

pub fn one(buf: &'static str) -> Mock {
    list(&[buf])
}

pub fn list(bufs: &[&'static str]) -> Mock {
    let mut polls = VecDeque::new();

    for &buf in bufs {
        polls.push_back(Ok(Ready(buf.as_bytes())));
    }

    Mock {
        polls,
        size_hint: SizeHint::default(),
    }
}

pub fn new_mock(values: &[Poll<&'static str, ()>]) -> Mock {
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
pub struct Mock {
    pub polls: VecDeque<Poll<&'static [u8], ()>>,
    pub size_hint: SizeHint,
}

#[derive(Debug)]
pub struct MockBuf {
    pub data: Cursor<&'static [u8]>,
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
