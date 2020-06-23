#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use std::io::Result;
use std::thread;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::try_join;

#[tokio::test]
async fn split() -> Result<()> {
    const MSG: &[u8] = b"split";

    let (stream1, mut stream2) = UnixStream::pair()?;

    let (_, mut write_half) = stream1.into_split();

    let ((), ()) = try_join! {
        async {
            let len = stream2.write(MSG).await?;
            assert_eq!(len, MSG.len());

            let mut read_buf = vec![0u8; 32];
            let read_len = stream2.read(&mut read_buf).await?;
            assert_eq!(&read_buf[..read_len], MSG);
            Result::Ok(())
        },
        async {
            let len = write_half.write(MSG).await?;
            assert_eq!(len, MSG.len());
            Ok(())
        },
    }?;

    Ok(())
}

#[tokio::test]
async fn reunite() -> Result<()> {
    let (stream1, stream2) = UnixStream::pair()?;

    let (read1, write1) = stream1.into_split();
    let (_, write2) = stream2.into_split();

    let read1 = match read1.reunite(write2) {
        Ok(_) => panic!("Reunite should not succeed"),
        Err(err) => err.0,
    };

    read1.reunite(write1).expect("Reunite should succeed");

    Ok(())
}

/// Test that dropping the write half actually closes the stream.
#[tokio::test]
async fn drop_write() -> Result<()> {
    const MSG: &[u8] = b"split";

    let (stream1, mut stream2) = UnixStream::pair()?;

    stream2.write(MSG).await?;

    let (mut read_half, write_half) = stream1.into_split();

    let mut read_buf = [0u8; 32];
    let read_len = read_half.read(&mut read_buf[..]).await?;
    assert_eq!(&read_buf[..read_len], MSG);

    // drop it while the read is in progress
    std::thread::spawn(move || {
        thread::sleep(std::time::Duration::from_millis(50));
        drop(write_half);
    });

    match read_half.read(&mut read_buf[..]).await {
        Ok(0) => {}
        Ok(len) => panic!("Unexpected read: {} bytes.", len),
        Err(err) => panic!("Unexpected error: {}.", err),
    }

    Ok(())
}
