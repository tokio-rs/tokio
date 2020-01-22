use tokio::sync::{mpsc, oneshot};
use tokio::task;
use tokio_test::{assert_ok, assert_pending, assert_ready};

use futures::future::poll_fn;
use std::task::Poll::Ready;

#[tokio::test]
async fn sync_one_lit_expr_comma() {
    let foo = tokio::select! {
        foo = async { 1 } => foo,
    };

    assert_eq!(foo, 1);
}

#[tokio::test]
async fn nested_one() {
    let foo = tokio::select! {
        foo = async { 1 } => tokio::select! {
            bar = async { foo } => bar,
        },
    };

    assert_eq!(foo, 1);
}

#[tokio::test]
async fn sync_one_lit_expr_no_comma() {
    let foo = tokio::select! {
        foo = async { 1 } => foo
    };

    assert_eq!(foo, 1);
}

#[tokio::test]
async fn sync_one_lit_expr_block() {
    let foo = tokio::select! {
        foo = async { 1 } => { foo }
    };

    assert_eq!(foo, 1);
}

#[tokio::test]
async fn sync_one_await() {
    let foo = tokio::select! {
        foo = one() => foo,
    };

    assert_eq!(foo, 1);
}

#[tokio::test]
async fn sync_one_ident() {
    let one = one();

    let foo = tokio::select! {
        foo = one => foo,
    };

    assert_eq!(foo, 1);
}

#[tokio::test]
async fn sync_two() {
    use std::cell::Cell;

    let cnt = Cell::new(0);

    let res = tokio::select! {
        foo = async {
            cnt.set(cnt.get() + 1);
            1
        } => foo,
        bar = async {
            cnt.set(cnt.get() + 1);
            2
        } => bar,
    };

    assert_eq!(1, cnt.get());
    assert!(res == 1 || res == 2);
}

#[tokio::test]
async fn drop_in_fut() {
    let s = "hello".to_string();

    let res = tokio::select! {
        foo = async {
            let v = one().await;
            drop(s);
            v
        } => foo
    };

    assert_eq!(res, 1);
}

#[tokio::test]
async fn one_ready() {
    let (tx1, rx1) = oneshot::channel::<i32>();
    let (_tx2, rx2) = oneshot::channel::<i32>();

    tx1.send(1).unwrap();

    let v = tokio::select! {
        res = rx1 => {
            assert_ok!(res)
        },
        _ = rx2 => unreachable!(),
    };

    assert_eq!(1, v);
}

#[tokio::test]
async fn select_streams() {
    let (tx1, mut rx1) = mpsc::unbounded_channel::<i32>();
    let (tx2, mut rx2) = mpsc::unbounded_channel::<i32>();

    tokio::spawn(async move {
        assert_ok!(tx2.send(1));
        task::yield_now().await;

        assert_ok!(tx1.send(2));
        task::yield_now().await;

        assert_ok!(tx2.send(3));
        task::yield_now().await;

        drop((tx1, tx2));
    });

    let mut rem = true;
    let mut msgs = vec![];

    while rem {
        tokio::select! {
            Some(x) = rx1.recv() => {
                msgs.push(x);
            }
            Some(y) = rx2.recv() => {
                msgs.push(y);
            }
            else => {
                rem = false;
            }
        }
    }

    msgs.sort();
    assert_eq!(&msgs[..], &[1, 2, 3]);
}

#[tokio::test]
async fn move_uncompleted_futures() {
    let (tx1, mut rx1) = oneshot::channel::<i32>();
    let (tx2, mut rx2) = oneshot::channel::<i32>();

    tx1.send(1).unwrap();
    tx2.send(2).unwrap();

    let ran;

    tokio::select! {
        res = &mut rx1 => {
            assert_eq!(1, assert_ok!(res));
            assert_eq!(2, assert_ok!(rx2.await));
            ran = true;
        },
        res = &mut rx2 => {
            assert_eq!(2, assert_ok!(res));
            assert_eq!(1, assert_ok!(rx1.await));
            ran = true;
        },
    }

    assert!(ran);
}

#[tokio::test]
async fn nested() {
    let res = tokio::select! {
        x = async { 1 } => {
            tokio::select! {
                y = async { 2 } => x + y,
            }
        }
    };

    assert_eq!(res, 3);
}

#[tokio::test]
async fn struct_size() {
    use futures::future;
    use std::mem;

    let fut = async {
        let ready = future::ready(0i32);

        tokio::select! {
            _ = ready => {},
        }
    };

    assert!(mem::size_of_val(&fut) <= 32);

    let fut = async {
        let ready1 = future::ready(0i32);
        let ready2 = future::ready(0i32);

        tokio::select! {
            _ = ready1 => {},
            _ = ready2 => {},
        }
    };

    assert!(mem::size_of_val(&fut) <= 40);

    let fut = async {
        let ready1 = future::ready(0i32);
        let ready2 = future::ready(0i32);
        let ready3 = future::ready(0i32);

        tokio::select! {
            _ = ready1 => {},
            _ = ready2 => {},
            _ = ready3 => {},
        }
    };

    assert!(mem::size_of_val(&fut) <= 48);
}

#[tokio::test]
async fn mutable_borrowing_future_with_same_borrow_in_block() {
    let mut value = 234;

    tokio::select! {
        _ = require_mutable(&mut value) => { },
        _ = async_noop() => {
            value += 5;
        },
    }

    assert!(value >= 234);
}

#[tokio::test]
async fn mutable_borrowing_future_with_same_borrow_in_block_and_else() {
    let mut value = 234;

    tokio::select! {
        _ = require_mutable(&mut value) => { },
        _ = async_noop() => {
            value += 5;
        },
        else => {
            value += 27;
        },
    }

    assert!(value >= 234);
}

#[tokio::test]
async fn future_panics_after_poll() {
    use tokio_test::task;

    let (tx, rx) = oneshot::channel();

    let mut polled = false;

    let f = poll_fn(|_| {
        assert!(!polled);
        polled = true;
        Ready(None::<()>)
    });

    let mut f = task::spawn(async {
        tokio::select! {
            Some(_) = f => unreachable!(),
            ret = rx => ret.unwrap(),
        }
    });

    assert_pending!(f.poll());
    assert_pending!(f.poll());

    assert_ok!(tx.send(1));

    let res = assert_ready!(f.poll());
    assert_eq!(1, res);
}

#[tokio::test]
async fn disable_with_if() {
    use tokio_test::task;

    let f = poll_fn(|_| panic!());
    let (tx, rx) = oneshot::channel();

    let mut f = task::spawn(async {
        tokio::select! {
            _ = f, if false => unreachable!(),
            _ = rx => (),
        }
    });

    assert_pending!(f.poll());

    assert_ok!(tx.send(()));
    assert!(f.is_woken());

    assert_ready!(f.poll());
}

#[tokio::test]
async fn join_with_select() {
    use tokio_test::task;

    let (tx1, mut rx1) = oneshot::channel();
    let (tx2, mut rx2) = oneshot::channel();

    let mut f = task::spawn(async {
        let mut a = None;
        let mut b = None;

        while a.is_none() || b.is_none() {
            tokio::select! {
                v1 = &mut rx1, if a.is_none() => a = Some(assert_ok!(v1)),
                v2 = &mut rx2, if b.is_none() => b = Some(assert_ok!(v2))
            }
        }

        (a.unwrap(), b.unwrap())
    });

    assert_pending!(f.poll());

    assert_ok!(tx1.send(123));
    assert!(f.is_woken());
    assert_pending!(f.poll());

    assert_ok!(tx2.send(456));
    assert!(f.is_woken());
    let (a, b) = assert_ready!(f.poll());

    assert_eq!(a, 123);
    assert_eq!(b, 456);
}

#[tokio::test]
async fn use_future_in_if_condition() {
    use tokio::time::{self, Duration};

    let mut delay = time::delay_for(Duration::from_millis(50));

    tokio::select! {
        _ = &mut delay, if !delay.is_elapsed() => {
        }
        _ = async { 1 } => {
        }
    }
}

#[tokio::test]
async fn many_branches() {
    let num = tokio::select! {
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
        x = async { 1 } => x,
    };

    assert_eq!(1, num);
}

async fn one() -> usize {
    1
}

async fn require_mutable(_: &mut i32) {}
async fn async_noop() {}
