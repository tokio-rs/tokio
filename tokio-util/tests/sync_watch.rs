#![warn(rust_2018_idioms)]

use tokio::sync::watch;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};
use tokio_util::stream::Stream;

use futures::prelude::*;

#[test]
fn stream_impl() {
    let (tx, rx) = watch::channel("one");
    let mut rx = rx.into_std();

    {
        let mut t = spawn(rx.next());
        let v = assert_ready!(t.poll()).unwrap();
        assert_eq!(v, "one");
    }

    {
        let mut t = spawn(rx.next());

        assert_pending!(t.poll());

        tx.broadcast("two").unwrap();

        assert!(t.is_woken());

        let v = assert_ready!(t.poll()).unwrap();
        assert_eq!(v, "two");
    }

    {
        let mut t = spawn(rx.next());

        assert_pending!(t.poll());

        drop(tx);

        let res = assert_ready!(t.poll());
        assert!(res.is_none());
    }
}
