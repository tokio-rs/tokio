#![cfg(feature = "process")]
#![warn(rust_2018_idioms)]
#![cfg(target_os = "linux")]
#![cfg(not(miri))]

use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{timeout, Duration};

// A read on a packet mode pipe returns a single packet, so a read that is shorter than the buffer
// does not mean that the pipe is drained.
#[tokio::test]
async fn issue_7051() {
    const BYTES_TO_WRITE: usize = 65536 * 2;
    const READ_BLOCK_SIZE: usize = 65536;

    let mut child = match Command::new("dd")
        .arg("if=/dev/zero")
        // dd sets O_DIRECT on its stdout, which makes the pipe a packet mode pipe
        .arg("oflag=direct")
        .arg(format!("bs={BYTES_TO_WRITE}"))
        .arg("count=1")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return, // dd is not available
    };

    let mut stdout = child.stdout.take().unwrap();
    // The buffer has to be larger than a packet, or the rest of the packet is discarded.
    let mut buffer = [0u8; READ_BLOCK_SIZE];
    let mut bytes_read = 0;
    loop {
        let n = timeout(Duration::from_secs(10), stdout.read(&mut buffer))
            .await
            .expect("the read got stuck")
            .unwrap();
        if n == 0 {
            break;
        }
        bytes_read += n;
    }

    assert_eq!(bytes_read, BYTES_TO_WRITE);
    child.wait().await.unwrap();
}
