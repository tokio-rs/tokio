use std::io;

#[cfg(windows)]
async fn windows_main() -> io::Result<()> {
    use std::time::Duration;
    use tokio::io::AsyncWriteExt as _;
    use tokio::io::{AsyncBufReadExt as _, BufReader};
    use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};
    use tokio::time;
    use winapi::shared::winerror;

    const PIPE_NAME: &str = r"\\.\pipe\named-pipe-multi-client";
    const N: usize = 10;

    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    // The first server needs to be constructed early so that clients can
    // be correctly connected. Otherwise a waiting client will error.
    //
    // Here we also make use of `first_pipe_instance`, which will ensure
    // that there are no other servers up and running already.
    let mut server = server_builder.clone().first_pipe_instance(true).create()?;

    let server = tokio::spawn(async move {
        // Artificial workload.
        time::sleep(Duration::from_secs(1)).await;

        for _ in 0..N {
            // Wait for client to connect.
            server.connect().await?;
            let mut inner = BufReader::new(server);

            // Construct the next server to be connected before sending the one
            // we already have of onto a task. This ensures that the server
            // isn't closed (after it's done in the task) before a new one is
            // available. Otherwise the client might error with
            // `io::ErrorKind::NotFound`.
            server = server_builder.create()?;

            let _ = tokio::spawn(async move {
                let mut buf = String::new();
                inner.read_line(&mut buf).await?;
                inner.write_all(b"pong\n").await?;
                inner.flush().await?;
                Ok::<_, io::Error>(())
            });
        }

        Ok::<_, io::Error>(())
    });

    let mut clients = Vec::new();

    for _ in 0..N {
        let client_builder = client_builder.clone();

        clients.push(tokio::spawn(async move {
            let client = loop {
                match client_builder.create() {
                    Ok(client) => break client,
                    Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
                    Err(e) => return Err(e),
                }

                // This showcases a generic connect loop.
                //
                // We immediately try to create a client, if it's not found or
                // the pipe is busy we use the specialized wait function on the
                // client builder.
                client_builder.wait(Some(Duration::from_secs(5))).await?;
            };

            let mut client = BufReader::new(client);

            let mut buf = String::new();
            client.write_all(b"ping\n").await?;
            client.flush().await?;
            client.read_line(&mut buf).await?;
            Ok::<_, io::Error>(buf)
        }));
    }

    for client in clients {
        let result = client.await?;
        assert_eq!(result?, "pong\n");
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
