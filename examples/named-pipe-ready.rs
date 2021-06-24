use std::io;

#[cfg(windows)]
async fn windows_main() -> io::Result<()> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Interest};
    use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};

    const PIPE_NAME: &str = r"\\.\pipe\named-pipe-single-client";

    let server = ServerOptions::new().create(PIPE_NAME)?;

    let server = tokio::spawn(async move {
        // Note: we wait for a client to connect.
        server.connect().await?;

        let mut server = BufReader::new(server);

        let mut buf = String::new();
        server.read_line(&mut buf).await?;
        server.write_all(b"pong\n").await?;
        Ok::<_, io::Error>(buf)
    });

    let client = tokio::spawn(async move {
        // There's no need to use a connect loop here, since we know that the
        // server is already up - `open` was called before spawning any of the
        // tasks.
        let client = ClientOptions::new().open(PIPE_NAME)?;

        let mut read_buf = [0u8; 5];
        let mut read_buf_cursor = 0;
        let write_buf = b"ping\n";
        let mut write_buf_cursor = 0;

        loop {
            let ready = client
                .ready(Interest::READABLE | Interest::WRITABLE)
                .await?;

            if ready.is_readable() {
                let buf = &mut read_buf[read_buf_cursor..];

                match client.try_read(buf) {
                    Ok(n) => {
                        read_buf_cursor += n;

                        if read_buf_cursor == read_buf.len() {
                            break;
                        }
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            if ready.is_writable() {
                let buf = &write_buf[write_buf_cursor..];

                if buf.is_empty() {
                    continue;
                }

                match client.try_write(buf) {
                    Ok(n) => {
                        write_buf_cursor += n;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                        continue;
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
        }

        let buf = String::from_utf8_lossy(&read_buf).into_owned();

        Ok::<_, io::Error>(buf)
    });

    let (server, client) = tokio::try_join!(server, client)?;

    assert_eq!(server?, "ping\n");
    assert_eq!(client?, "pong\n");

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
