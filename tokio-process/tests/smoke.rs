extern crate tokio_process;

use tokio_process::CommandExt;

mod support;

#[test]
fn simple() {
    let mut cmd = support::cmd("exit");
    cmd.arg("2");

    let mut child = cmd.spawn_async().unwrap();

    let id = child.id();
    assert!(id > 0);

    let status = support::run_with_timeout(&mut child)
        .expect("failed to run future");
    assert_eq!(status.code(), Some(2));

    assert_eq!(child.id(), id);
    drop(child.kill());
}
