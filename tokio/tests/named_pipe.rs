#![cfg(feature = "full")]
#![cfg(all(windows))]

use std::io;
use std::mem;
use std::os::windows::io::AsRawHandle;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _, ReadBuf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipe, PipeMode, ServerOptions};
use tokio::time;
use winapi::shared::winerror;

#[tokio::test]
async fn test_named_pipe_peek_consumed() -> io::Result<()> {
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

    for _ in 0..N {
        if available < buf.len() {
            server.read_exact(&mut buf).await?;
            assert_eq!(&buf[..], b"ping");

            let info = server.peek(Some(&mut ReadBuf::new(&mut buf[..])))?;
            available = info.total_bytes_available;
            continue;
        }

        server.read_exact(&mut buf).await?;
        available -= buf.len();
        assert_eq!(&buf[..], b"ping");
    }

    server.write_all(b"pong").await?;

    let buf = client.await??;
    assert_eq!(&buf[..], b"pong");
    Ok(())
}

async fn peek_ping_pong(n: usize, mut client: NamedPipe, mut server: NamedPipe) -> io::Result<()> {
    use rand::Rng as _;
    use std::sync::Arc;

    /// A weirdly sized read to induce as many partial reads as possible.
    const UNALIGNED: usize = 27;

    let mut data = vec![0; 1024];

    let mut r = rand::thread_rng();
    r.fill(&mut data[..]);

    let data = Arc::new(data);
    let data2 = data.clone();

    let client = tokio::spawn(async move {
        for _ in 0..n {
            client.write_all(&data2[..]).await?;
        }

        let mut buf = [0u8; 4];
        client.read_exact(&mut buf).await?;
        Ok::<_, io::Error>(buf)
    });

    let server = tokio::spawn(async move {
        let mut full_buf = vec![0u8; data.len()];
        let mut peeks = 0u32;

        for _ in 0..n {
            let mut buf = &mut full_buf[..];

            while !buf.is_empty() {
                let e = usize::min(buf.len(), UNALIGNED);
                let r = server.read(&mut buf[..e]).await?;
                buf = &mut buf[r..];

                let info = server.peek(None)?;

                if info.total_bytes_available != 0 {
                    peeks += 1;
                }
            }

            assert_eq!(&full_buf[..], &data[..]);
        }

        server.write_all(b"pong").await?;

        // NB: this is not at all helpful, but keeping it here because when run
        // in release mode we can usually see a couple of hundred peeks go
        // through.
        assert!(peeks == 0 || peeks > 0);
        Ok::<_, io::Error>(())
    });

    let (client, server) = tokio::try_join!(client, server)?;
    assert_eq!(&client?[..], b"pong");
    let _ = server?;
    Ok(())
}

#[tokio::test]
async fn test_named_pipe_peek() -> io::Result<()> {
    {
        const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-server-peek-small";

        let server = ServerOptions::new().create(PIPE_NAME)?;
        let client = ClientOptions::new().open(PIPE_NAME)?;
        server.connect().await?;

        peek_ping_pong(1, client, server).await?;
    }

    {
        const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-peek-big";

        let server = ServerOptions::new().create(PIPE_NAME)?;
        let client = ClientOptions::new().open(PIPE_NAME)?;
        server.connect().await?;

        peek_ping_pong(1, server, client).await?;
    }

    {
        const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-server-peek-big";

        let server = ServerOptions::new().create(PIPE_NAME)?;
        let client = ClientOptions::new().open(PIPE_NAME)?;
        server.connect().await?;

        peek_ping_pong(100, client, server).await?;
    }

    {
        const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-peek-big";

        let server = ServerOptions::new().create(PIPE_NAME)?;
        let client = ClientOptions::new().open(PIPE_NAME)?;
        server.connect().await?;

        peek_ping_pong(100, server, client).await?;
    }

    Ok(())
}

#[tokio::test]
async fn test_named_pipe_client_drop() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-drop";

    let mut server = ServerOptions::new().create(PIPE_NAME)?;

    assert_eq!(num_instances("test-named-pipe-client-drop")?, 1);

    let client = ClientOptions::new().open(PIPE_NAME)?;

    server.connect().await?;
    drop(client);

    // instance will be broken because client is gone
    match server.write_all(b"ping").await {
        Err(e) if e.raw_os_error() == Some(winerror::ERROR_NO_DATA as i32) => (),
        x => panic!("{:?}", x),
    }

    Ok(())
}

// This tests what happens when a client tries to disconnect.
#[tokio::test]
async fn test_named_pipe_client_connect() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-connect";

    let server = ServerOptions::new().create(PIPE_NAME)?;
    let client = ClientOptions::new().open(PIPE_NAME)?;
    server.connect().await?;

    let e = client.connect().await.unwrap_err();
    assert_eq!(
        e.raw_os_error(),
        Some(winerror::ERROR_INVALID_FUNCTION as i32)
    );
    Ok(())
}

#[tokio::test]
async fn test_named_pipe_single_client() -> io::Result<()> {
    use tokio::io::{AsyncBufReadExt as _, BufReader};

    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-single-client";

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
        let client = ClientOptions::new().open(PIPE_NAME)?;

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

#[tokio::test]
async fn test_named_pipe_multi_client() -> io::Result<()> {
    use tokio::io::{AsyncBufReadExt as _, BufReader};

    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-multi-client";
    const N: usize = 10;

    // The first server needs to be constructed early so that clients can
    // be correctly connected. Otherwise calling .wait will cause the client to
    // error.
    let mut server = ServerOptions::new().create(PIPE_NAME)?;

    let server = tokio::spawn(async move {
        for _ in 0..N {
            // Wait for client to connect.
            server.connect().await?;
            let mut inner = BufReader::new(server);

            // Construct the next server to be connected before sending the one
            // we already have of onto a task. This ensures that the server
            // isn't closed (after it's done in the task) before a new one is
            // available. Otherwise the client might error with
            // `io::ErrorKind::NotFound`.
            server = ServerOptions::new().create(PIPE_NAME)?;

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
        clients.push(tokio::spawn(async move {
            // This showcases a generic connect loop.
            //
            // We immediately try to create a client, if it's not found or the
            // pipe is busy we use the specialized wait function on the client
            // builder.
            let client = loop {
                match ClientOptions::new().open(PIPE_NAME) {
                    Ok(client) => break client,
                    Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }

                // Wait for a named pipe to become available.
                time::sleep(Duration::from_millis(50)).await;
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

// This tests what happens when a client tries to disconnect.
#[tokio::test]
async fn test_named_pipe_mode_message() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-mode-message";

    let server = ServerOptions::new()
        .pipe_mode(PipeMode::Message)
        .create(PIPE_NAME)?;

    let _ = ClientOptions::new().open(PIPE_NAME)?;
    server.connect().await?;
    Ok(())
}

fn num_instances(pipe_name: impl AsRef<str>) -> io::Result<u32> {
    use ntapi::ntioapi;
    use winapi::shared::ntdef;

    let mut name = pipe_name.as_ref().encode_utf16().collect::<Vec<_>>();
    let mut name = ntdef::UNICODE_STRING {
        Length: (name.len() * mem::size_of::<u16>()) as u16,
        MaximumLength: (name.len() * mem::size_of::<u16>()) as u16,
        Buffer: name.as_mut_ptr(),
    };
    let root = std::fs::File::open(r"\\.\Pipe\")?;
    let mut io_status_block = unsafe { mem::zeroed() };
    let mut file_directory_information = [0_u8; 1024];

    let status = unsafe {
        ntioapi::NtQueryDirectoryFile(
            root.as_raw_handle(),
            std::ptr::null_mut(),
            None,
            std::ptr::null_mut(),
            &mut io_status_block,
            &mut file_directory_information as *mut _ as *mut _,
            1024,
            ntioapi::FileDirectoryInformation,
            0,
            &mut name,
            0,
        )
    };

    if status as u32 != winerror::NO_ERROR {
        return Err(io::Error::last_os_error());
    }

    let info = unsafe {
        mem::transmute::<_, &ntioapi::FILE_DIRECTORY_INFORMATION>(&file_directory_information)
    };
    let raw_name = unsafe {
        std::slice::from_raw_parts(
            info.FileName.as_ptr(),
            info.FileNameLength as usize / mem::size_of::<u16>(),
        )
    };
    let name = String::from_utf16(raw_name).unwrap();
    let num_instances = unsafe { *info.EndOfFile.QuadPart() };

    assert_eq!(name, pipe_name.as_ref());

    Ok(num_instances as u32)
}
