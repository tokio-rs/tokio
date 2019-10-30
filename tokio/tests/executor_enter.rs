#![warn(rust_2018_idioms)]

#[test]
fn block_on_ready() {
    let mut enter = tokio::executor::enter().unwrap();
    let val = enter.block_on(async { 123 });

    assert_eq!(val, 123);
}

#[test]
fn block_on_pending() {
    let mut enter = tokio::executor::enter().unwrap();
    let val = enter.block_on(async { 123 });

    assert_eq!(val, 123);
}
