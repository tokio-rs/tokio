use std::io;
use tokio::io::AsyncWriteExt as _;
use tokio::io::{AsyncBufReadExt as _, BufReader};
use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};
use winapi::shared::winerror;

const PIPE_NAME: &str = r"\\.\pipe\named-pipe-single-client";
const N: usize = 10;

#[tokio::main]
async fn main() -> io::Result<()> {
    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let server = tokio::spawn(async move {
        for _ in 0..N {
            let server = server_builder.create()?;
            // Wait for client to connect.
            server.connect().await?;
            let mut server = BufReader::new(server);

            let _ = tokio::spawn(async move {
                let mut buf = String::new();
                server.read_line(&mut buf).await?;
                server.write_all(b"pong\n").await?;
                server.flush().await?;
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
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }

                // This showcases a generic connect loop.
                //
                // We immediately try to create a client, if it's not found or
                // the pipe is busy we use the specialized wait function on the
                // client builder.
                client_builder.wait(None).await?;
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
