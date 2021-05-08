use std::io;

#[cfg(windows)]
async fn windows_main() -> io::Result<()> {
    use tokio::io::AsyncWriteExt as _;
    use tokio::io::{AsyncBufReadExt as _, BufReader};
    use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};

    const PIPE_NAME: &str = r"\\.\pipe\named-pipe-single-client";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let server = server_builder.create()?;

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
        client_builder.wait(None).await?;
        let client = client_builder.create()?;

        let mut client = BufReader::new(client);

        let mut buf = String::new();
        client.write_all(b"ping\n").await?;
        client.read_line(&mut buf).await?;
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
