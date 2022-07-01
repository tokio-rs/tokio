//! A cat-like utility that can be used as a subprocess to test I/O
//! stream communication.

use tokio::io::AsyncWriteExt;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();

    tokio::io::copy(&mut stdin, &mut stdout).await.unwrap();

    stdout.flush().await.unwrap();
}
