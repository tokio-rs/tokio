#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::process::Command;
use tokio_test::assert_ok;

macro_rules! cmd {
    () => {{
        let mut cmd;

        if cfg!(windows) {
            cmd = Command::new("cmd");
            cmd.arg("/c");
        } else {
            cmd = Command::new("sh");
            cmd.arg("-c");
        }

        cmd
    }};
}

#[tokio::test]
async fn simple() {
    let mut cmd = cmd!();
    let mut child = cmd.arg("exit 2").spawn().unwrap();

    let id = child.id();
    assert!(id > 0);

    let status = assert_ok!((&mut child).await);
    assert_eq!(status.code(), Some(2));

    assert_eq!(child.id(), id);
    drop(child.kill());
}

#[tokio::test]
async fn kill_handle() {
    let mut cmd = cmd!();
    let mut child = cmd.arg("sleep 10").spawn().unwrap();

    let mut kill_handle = child.kill_handle();

    let join = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        kill_handle.kill().expect("kill failed");
    });

    let id = child.id();
    assert!(id > 0);

    let status = assert_ok!((&mut child).await);
    assert!(!status.success());

    join.join().unwrap();
}
