use std::io;

#[cfg(windows)]
async fn windows_main() -> io::Result<()> {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

    const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-peek-consumed";
    const N: usize = 1000;

    let mut server = ServerOptions::new().create(PIPE_NAME)?;
    let mut client = ClientOptions::new().open(PIPE_NAME)?;
    server.connect().await?;

    let client = tokio::spawn(async move {
        for _ in 0..N {
            client.write_all(b"ping").await?;
        }

        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).await?;

        Ok::<_, io::Error>(buf)
    });

    let mut buf = [0u8; 4];
    let mut available = 0;

    for n in 0..N {
        if available < 4 {
            println!("read_exact: (n: {}, available: {})", n, available);
            server.read_exact(&mut buf).await?;
            assert_eq!(&buf[..], b"ping");

            let info = server.peek(None)?;
            available = info.total_bytes_available;
            continue;
        }

        println!("read_exact: (n: {}, available: {})", n, available);
        server.read_exact(&mut buf).await?;
        available -= buf.len();
        assert_eq!(&buf[..], b"ping");
    }

    server.write_all(b"pong").await?;

    let buf = client.await??;
    assert_eq!(&buf[..], b"pong");
    Ok(())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    #[cfg(windows)]
    {
        windows_main().await?;
    }

    #[cfg(not(windows))]
    {
        println!("Named pipes are only supported on Windows!");
    }

    Ok(())
}
