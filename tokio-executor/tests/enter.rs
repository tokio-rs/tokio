#![warn(rust_2018_idioms)]
#![feature(async_await)]

#[test]
fn block_on_ready() {
    let mut enter = tokio_executor::enter().unwrap();
    let val = enter.block_on(async { 123 });

    assert_eq!(val, 123);
}

#[test]
fn block_on_pending() {
    let mut enter = tokio_executor::enter().unwrap();
    let val = enter.block_on(async { 123 });

    assert_eq!(val, 123);
}
