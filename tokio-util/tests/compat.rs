#![cfg(feature = "compat")]
#![warn(rust_2018_idioms)]

use futures_util::io;

async fn tokio_stream_position() -> io::Result<()> {
    use tokio::fs::File;
    use tokio::io::{AsyncSeekExt, AsyncWriteExt};
    let path = "../target/tokio-seek.test";

    let mut file = File::create(path).await?;
    let buffer = [0u8; 16];
    file.write_all(&buffer).await?;

    let pos = file.stream_position().await?;
    println!("tokio got pos {}", pos);

    Ok(())
}

async fn compat_stream_position() -> io::Result<()> {
    use futures::io::{AsyncSeekExt, AsyncWriteExt};
    use tokio::fs::File;
    use tokio_util::compat::TokioAsyncWriteCompatExt;
    let path = "../target/compat-seek.test";

    let mut file = File::create(path).await?.compat_write();
    let buffer = [0u8; 16];
    file.write_all(&buffer).await?;

    // Error: other file operation is pending,
    // call poll_complete before start_seek
    let pos = file.stream_position().await?;
    println!("futures got pos {}", pos);

    Ok(())
}

#[tokio::test]
async fn combined() -> io::Result<()> {
    tokio_stream_position().await?;
    compat_stream_position().await?;
    Ok(())
}
