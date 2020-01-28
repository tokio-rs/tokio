use tokio::sync::oneshot;
use tokio_test::{assert_pending, assert_ready, task};

#[tokio::test]
async fn sync_one_lit_expr_comma() {
    let foo = tokio::join!(async { 1 },);

    assert_eq!(foo, (1,));
}

#[tokio::test]
async fn sync_one_lit_expr_no_comma() {
    let foo = tokio::join!(async { 1 });

    assert_eq!(foo, (1,));
}

#[tokio::test]
async fn sync_two_lit_expr_comma() {
    let foo = tokio::join!(async { 1 }, async { 2 },);

    assert_eq!(foo, (1, 2));
}

#[tokio::test]
async fn sync_two_lit_expr_no_comma() {
    let foo = tokio::join!(async { 1 }, async { 2 });

    assert_eq!(foo, (1, 2));
}

#[tokio::test]
async fn two_await() {
    let (tx1, rx1) = oneshot::channel::<&str>();
    let (tx2, rx2) = oneshot::channel::<u32>();

    let mut join = task::spawn(async {
        tokio::join!(async { rx1.await.unwrap() }, async { rx2.await.unwrap() })
    });

    assert_pending!(join.poll());

    tx2.send(123).unwrap();
    assert!(join.is_woken());
    assert_pending!(join.poll());

    tx1.send("hello").unwrap();
    assert!(join.is_woken());
    let res = assert_ready!(join.poll());

    assert_eq!(("hello", 123), res);
}

#[test]
fn join_size() {
    use futures::future;
    use std::mem;

    let fut = async {
        let ready = future::ready(0i32);
        tokio::join!(ready)
    };
    assert_eq!(mem::size_of_val(&fut), 16);

    let fut = async {
        let ready1 = future::ready(0i32);
        let ready2 = future::ready(0i32);
        tokio::join!(ready1, ready2)
    };
    assert_eq!(mem::size_of_val(&fut), 28);
}
