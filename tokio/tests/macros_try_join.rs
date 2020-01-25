use tokio::sync::oneshot;
use tokio_test::{assert_pending, assert_ready, task};

#[tokio::test]
async fn sync_one_lit_expr_comma() {
    let foo = tokio::try_join!(async { ok(1) },);

    assert_eq!(foo, Ok((1,)));
}

#[tokio::test]
async fn sync_one_lit_expr_no_comma() {
    let foo = tokio::try_join!(async { ok(1) });

    assert_eq!(foo, Ok((1,)));
}

#[tokio::test]
async fn sync_two_lit_expr_comma() {
    let foo = tokio::try_join!(async { ok(1) }, async { ok(2) },);

    assert_eq!(foo, Ok((1, 2)));
}

#[tokio::test]
async fn sync_two_lit_expr_no_comma() {
    let foo = tokio::try_join!(async { ok(1) }, async { ok(2) });

    assert_eq!(foo, Ok((1, 2)));
}

#[tokio::test]
async fn two_await() {
    let (tx1, rx1) = oneshot::channel::<&str>();
    let (tx2, rx2) = oneshot::channel::<u32>();

    let mut join =
        task::spawn(async { tokio::try_join!(async { rx1.await }, async { rx2.await }) });

    assert_pending!(join.poll());

    tx2.send(123).unwrap();
    assert!(join.is_woken());
    assert_pending!(join.poll());

    tx1.send("hello").unwrap();
    assert!(join.is_woken());
    let res: Result<(&str, u32), _> = assert_ready!(join.poll());

    assert_eq!(Ok(("hello", 123)), res);
}

#[tokio::test]
async fn err_abort_early() {
    let (tx1, rx1) = oneshot::channel::<&str>();
    let (tx2, rx2) = oneshot::channel::<u32>();
    let (_tx3, rx3) = oneshot::channel::<u32>();

    let mut join = task::spawn(async {
        tokio::try_join!(async { rx1.await }, async { rx2.await }, async {
            rx3.await
        })
    });

    assert_pending!(join.poll());

    tx2.send(123).unwrap();
    assert!(join.is_woken());
    assert_pending!(join.poll());

    drop(tx1);
    assert!(join.is_woken());

    let res = assert_ready!(join.poll());

    assert!(res.is_err());
}

#[test]
fn join_size() {
    use futures::future;
    use std::mem;

    let fut = async {
        let ready = future::ready(ok(0i32));
        tokio::try_join!(ready)
    };
    assert_eq!(mem::size_of_val(&fut), 16);

    let fut = async {
        let ready1 = future::ready(ok(0i32));
        let ready2 = future::ready(ok(0i32));
        tokio::try_join!(ready1, ready2)
    };
    assert_eq!(mem::size_of_val(&fut), 28);
}

fn ok<T>(val: T) -> Result<T, ()> {
    Ok(val)
}
