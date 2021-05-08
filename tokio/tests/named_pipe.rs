#![cfg(feature = "full")]
#![cfg(all(windows))]

use std::io;
use std::mem;
use std::os::windows::io::AsRawHandle;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder, PipeEnd, PipeMode};
use winapi::shared::winerror;

#[tokio::test]
async fn test_named_pipe_permissions() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-permissions";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    // Server side prevents connecting by denying inbound access, client errors
    // when attempting to create the connection.
    {
        let server_builder = server_builder.clone().access_inbound(false);
        let _server = server_builder.create()?;

        let e = client_builder.create().unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);

        // Disabling reading allows a client to connect, but leads to runtime
        // error if a read is attempted.
        let mut client = client_builder.clone().write(false).create()?;

        let e = client.write(b"ping").await.unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    }

    // Server side prevents connecting by denying outbound access, client errors
    // when attempting to create the connection.
    {
        let server_builder = server_builder.clone().access_outbound(false);
        let _server = server_builder.create()?;

        let e = client_builder.create().unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);

        // Disabling reading allows a client to connect, but leads to runtime
        // error if a read is attempted.
        let mut client = client_builder.clone().read(false).create()?;

        let mut buf = [0u8; 4];
        let e = client.read(&mut buf).await.unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    }

    // A functional, unidirectional server-to-client only communication.
    {
        let mut server = server_builder.clone().access_inbound(false).create()?;
        let mut client = client_builder.clone().write(false).create()?;

        let write = server.write_all(b"ping");

        let mut buf = [0u8; 4];
        let read = client.read_exact(&mut buf);

        let ((), read) = tokio::try_join!(write, read)?;

        assert_eq!(read, 4);
        assert_eq!(&buf[..], b"ping");
    }

    // A functional, unidirectional client-to-server only communication.
    {
        let mut server = server_builder.clone().access_outbound(false).create()?;
        let mut client = client_builder.clone().read(false).create()?;

        // TODO: Explain why this test doesn't work without calling connect
        // first.
        //
        // Because I have no idea -- udoprog
        server.connect().await?;

        let write = client.write_all(b"ping");

        let mut buf = [0u8; 4];
        let read = server.read_exact(&mut buf);

        let ((), read) = tokio::try_join!(write, read)?;

        println!("done reading and writing");

        assert_eq!(read, 4);
        assert_eq!(&buf[..], b"ping");
    }

    Ok(())
}

#[tokio::test]
async fn test_named_pipe_info() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-info";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME)
        .pipe_mode(PipeMode::Message)
        .max_instances(5);

    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let server = server_builder.create()?;
    let client = client_builder.create()?;

    let server_info = server.info()?;
    let client_info = client.info()?;

    assert_eq!(server_info.end, PipeEnd::Server);
    assert_eq!(server_info.mode, PipeMode::Message);
    assert_eq!(server_info.max_instances, 5);

    assert_eq!(client_info.end, PipeEnd::Client);
    assert_eq!(client_info.mode, PipeMode::Message);
    assert_eq!(server_info.max_instances, 5);

    Ok(())
}

#[tokio::test]
async fn test_named_pipe_client_drop() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-drop";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let mut server = server_builder.create()?;
    assert_eq!(num_instances("test-named-pipe-client-drop")?, 1);

    let client = client_builder.create()?;

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
async fn test_named_pipe_client_disconnect() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-disconnect";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let server = server_builder.create()?;
    let client = client_builder.create()?;
    server.connect().await?;

    let e = client.disconnect().unwrap_err();
    assert_eq!(
        e.raw_os_error(),
        Some(winerror::ERROR_INVALID_FUNCTION as i32)
    );
    Ok(())
}

// This tests what happens when a client tries to disconnect.
#[tokio::test]
async fn test_named_pipe_client_connect() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-client-connect";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let server = server_builder.create()?;
    let client = client_builder.create()?;
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

#[tokio::test]
async fn test_named_pipe_multi_client() -> io::Result<()> {
    use tokio::io::{AsyncBufReadExt as _, BufReader};

    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-multi-client";
    const N: usize = 10;

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
            // This showcases a generic connect loop.
            //
            // We immediately try to create a client, if it's not found or the
            // pipe is busy we use the specialized wait function on the client
            // builder.
            let client = loop {
                match client_builder.create() {
                    Ok(client) => break client,
                    Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => return Err(e),
                }

                // Wait for a named pipe to become available.
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

// This tests what happens when a client tries to disconnect.
#[tokio::test]
async fn test_named_pipe_mode_message() -> io::Result<()> {
    const PIPE_NAME: &str = r"\\.\pipe\test-named-pipe-mode-message";

    let server_builder = NamedPipeBuilder::new(PIPE_NAME).pipe_mode(PipeMode::Message);
    let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);

    let server = server_builder.create()?;
    let client = client_builder.create()?;
    server.connect().await?;

    let e = client.connect().await.unwrap_err();
    assert_eq!(
        e.raw_os_error(),
        Some(winerror::ERROR_INVALID_FUNCTION as i32)
    );
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
