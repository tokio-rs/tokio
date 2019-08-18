#![warn(rust_2018_idioms)]
#![feature(async_await)]

mod support;

#[tokio::test]
async fn simple() {
    let mut cmd = support::cmd("exit");
    cmd.arg("2");

    let mut child = cmd.spawn().unwrap();

    let id = child.id();
    assert!(id > 0);

    let status = support::with_timeout(&mut child)
        .await
        .expect("failed to run future");
    assert_eq!(status.code(), Some(2));

    assert_eq!(child.id(), id);
    drop(child.kill());
}
