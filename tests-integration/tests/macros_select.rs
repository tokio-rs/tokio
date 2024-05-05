#![cfg(feature = "macros")]

use futures::channel::oneshot;
use futures::executor::block_on;
use std::thread;

#[cfg_attr(
    not(feature = "rt-multi-thread"),
    ignore = "WASI: std::thread::spawn not supported"
)]
#[test]
fn join_with_select() {
    block_on(async {
        let (tx1, mut rx1) = oneshot::channel::<i32>();
        let (tx2, mut rx2) = oneshot::channel::<i32>();

        thread::spawn(move || {
            tx1.send(123).unwrap();
            tx2.send(456).unwrap();
        });

        let mut a = None;
        let mut b = None;

        while a.is_none() || b.is_none() {
            tokio::select! {
                v1 = (&mut rx1), if a.is_none() => a = Some(v1.unwrap()),
                v2 = (&mut rx2), if b.is_none() => b = Some(v2.unwrap()),
            }
        }

        let (a, b) = (a.unwrap(), b.unwrap());

        assert_eq!(a, 123);
        assert_eq!(b, 456);
    });
}
