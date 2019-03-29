#![cfg(feature = "util")]

extern crate bytes;
extern crate futures;
extern crate tokio_buf;

use futures::Async::*;
use tokio_buf::{BufStream, BufStreamExt};

#[macro_use]
mod support;

use support::*;

#[test]
fn chain() {
    // Chain one with one
    //
    let mut bs = one("hello").chain(one("world"));

    assert_buf_eq!(bs.poll_buf(), "hello");
    assert_buf_eq!(bs.poll_buf(), "world");
    assert_none!(bs.poll_buf());

    // Chain multi with multi
    let mut bs = list(&["foo", "bar"]).chain(list(&["baz", "bok"]));

    assert_buf_eq!(bs.poll_buf(), "foo");
    assert_buf_eq!(bs.poll_buf(), "bar");
    assert_buf_eq!(bs.poll_buf(), "baz");
    assert_buf_eq!(bs.poll_buf(), "bok");
    assert_none!(bs.poll_buf());

    // Chain includes a not ready call
    //
    let mut bs = new_mock(&[Ok(Ready("foo")), Ok(NotReady), Ok(Ready("bar"))]).chain(one("baz"));

    assert_buf_eq!(bs.poll_buf(), "foo");
    assert_not_ready!(bs.poll_buf());
    assert_buf_eq!(bs.poll_buf(), "bar");
    assert_buf_eq!(bs.poll_buf(), "baz");
    assert_none!(bs.poll_buf());
}
