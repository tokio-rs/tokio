#![warn(rust_2018_idioms)]

use tokio_sync::watch;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready};

fn main() {
    let (tx, mut rx1) = watch::channel("one");
    let mut rx2 = rx1.clone();

    {
        let mut t1 = spawn(rx1.recv_ref());
        let mut t2 = spawn(rx2.recv_ref());

        let res = assert_ready!(t1.poll());
        assert_eq!(*res.unwrap(), "one");

        let res = assert_ready!(t2.poll());
        assert_eq!(*res.unwrap(), "one");
    }

    let mut t2 = spawn(rx2.recv_ref());

    {
        let mut t1 = spawn(rx1.recv_ref());

        assert_pending!(t1.poll());
        assert_pending!(t2.poll());

        tx.broadcast("two").unwrap();

        assert!(t1.is_woken());
        assert!(t2.is_woken());

        let res = assert_ready!(t1.poll());
        assert_eq!(*res.unwrap(), "two");
    }

    {
        let mut t1 = spawn(rx1.recv_ref());

        assert_pending!(t1.poll());

        tx.broadcast("three").unwrap();

        assert!(t1.is_woken());
        assert!(t2.is_woken());

        let res = assert_ready!(t1.poll());
        assert_eq!(*res.unwrap(), "three");

        let res = assert_ready!(t2.poll());
        assert_eq!(*res.unwrap(), "three");
    }

    drop(t2);

    {
        let mut t1 = spawn(rx1.recv_ref());
        let mut t2 = spawn(rx2.recv_ref());

        assert_pending!(t1.poll());
        assert_pending!(t2.poll());

        tx.broadcast("four").unwrap();

        let res = assert_ready!(t1.poll());
        assert_eq!(*res.unwrap(), "four");
        drop(t1);

        let mut t1 = spawn(rx1.recv_ref());
        assert_pending!(t1.poll());

        drop(tx);

        assert!(t1.is_woken());
        let res = assert_ready!(t1.poll());
        assert!(res.is_none());

        let res = assert_ready!(t2.poll());
        assert_eq!(*res.unwrap(), "four");

        drop(t2);
        let mut t2 = spawn(rx2.recv_ref());
        let res = assert_ready!(t2.poll());
        assert!(res.is_none());
    }
}
