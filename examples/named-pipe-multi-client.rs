use std::io;

#[cfg(windows)]
async fn windows_main() -> io::Result<()> {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{ClientOptions, ServerOptions};
    use tokio::time;
    use windows_sys::Win32::Foundation::ERROR_PIPE_BUSY;

    const PIPE_NAME: &str = r"\\.\pipe\named-pipe-multi-client";
    const N: usize = 10;

    // The first server needs to be constructed early so that clients can
    // be correctly connected. Otherwise a waiting client will error.
    //
    // Here we also make use of `first_pipe_instance`, which will ensure
    // that there are no other servers up and running already.
    let mut server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(PIPE_NAME)?;

    let server = tokio::spawn(async move {
        // Artificial workload.
        time::sleep(Duration::from_secs(1)).await;

        for _ in 0..N {
            // Wait for client to connect.
            server.connect().await?;
            let mut inner = server;

            // Construct the next server to be connected before sending the one
            // we already have of onto a task. This ensures that the server
            // isn't closed (after it's done in the task) before a new one is
            // available. Otherwise the client might error with
            // `io::ErrorKind::NotFound`.
            server = ServerOptions::new().create(PIPE_NAME)?;

            let _ = tokio::spawn(async move {
                let mut buf = vec![0u8; 4];
                inner.read_exact(&mut buf).await?;
                inner.write_all(b"pong").await?;
                Ok::<_, io::Error>(())
            });
        }

        Ok::<_, io::Error>(())
    });

    let mut clients = Vec::new();

    for _ in 0..N {
        clients.push(tokio::spawn(async move {
            // This showcases a generic connect loop.
            //
            // We immediately try to create a client, if it's not found or
            // the pipe is busy we use the specialized wait function on the
            // client builder.
            let mut client = loop {
                match ClientOptions::new().open(PIPE_NAME) {
                    Ok(client) => break client,
                    Err(e) if e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32) => (),
                    Err(e) => return Err(e),
                }

                time::sleep(Duration::from_millis(5)).await;
            };

            let mut buf = [0u8; 4];
            client.write_all(b"ping").await?;
            client.read_exact(&mut buf).await?;
            Ok::<_, io::Error>(buf)
        }));
    }

    for client in clients {
        let result = client.await?;
        assert_eq!(&result?[..], b"pong");
    }

    server.await??;
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
