#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", unix))]
#![cfg(not(miri))] // Miri doesn't support pipe2

use tokio::process::Command;

#[tokio::test]
async fn arg0() {
    let mut cmd = Command::new("sh");
    cmd.arg0("test_string").arg("-c").arg("echo $0");

    let output = cmd.output().await.unwrap();
    assert_eq!(output.stdout, b"test_string\n");
}
