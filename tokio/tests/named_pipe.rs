#![cfg(feature = "full")]
#![warn(rust_2018_idioms)]
#![cfg(target_os = "windows")]

use std::io;
use std::mem::{size_of, transmute, zeroed};
use std::os::raw::c_void;
use std::os::windows::io::AsRawHandle;
use std::slice::from_raw_parts;

use bytes::Buf;
use futures::{
    future::{select, try_join, try_join_all, Either},
    stream::FuturesUnordered,
    StreamExt,
};
use ntapi::ntioapi::FileDirectoryInformation;
use ntapi::ntioapi::NtQueryDirectoryFile;
use ntapi::ntioapi::FILE_DIRECTORY_INFORMATION;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{NamedPipe, NamedPipeServerBuilder},
    time::{sleep, Duration},
};
use winapi::shared::ntdef::UNICODE_STRING;
use winapi::shared::winerror::*;

#[tokio::test]
async fn basic() -> io::Result<()> {
    const NUM_CLIENTS: u32 = 255;
    const PIPE_NAME: &'static str = r"\\.\pipe\test-named-pipe-basic";
    let mut buf = [0_u8; 16];

    // Create server to avoid NotFound from clients.
    let server = NamedPipeServerBuilder::new(PIPE_NAME).build()?;

    let server = async move {
        let mut pipe;
        for _ in 0..NUM_CLIENTS {
            pipe = server.accept().await?;
            let mut buf = Vec::new();
            pipe.read_buf(&mut buf).await?;
            let i = (&*buf).get_u32_le();
            pipe.write_all(format!("Server to {}", i).as_bytes())
                .await?;
        }
        std::io::Result::Ok(())
    };

    // concurrent clients
    let clients = (0..NUM_CLIENTS)
        .map(|i| async move {
            let mut pipe = NamedPipe::connect(PIPE_NAME).await?;
            pipe.write_all(&i.to_le_bytes()).await?;
            let mut buf = Vec::new();
            pipe.read_buf(&mut buf).await?;
            assert_eq!(buf, format!("Server to {}", i).as_bytes());
            std::io::Result::Ok(())
        })
        .collect::<FuturesUnordered<_>>()
        .fold(Ok(()), |a, x| async move { a.and(x) });

    try_join(server, clients).await?;

    // client returns not found if there is no server
    let err = NamedPipe::connect(PIPE_NAME).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::NotFound);

    let server = NamedPipeServerBuilder::new(PIPE_NAME).build()?.accept();
    let client = NamedPipe::connect(PIPE_NAME);
    let (mut server, mut client) = try_join(server, client).await?;

    ping_pong(&mut server, &mut client).await?;

    drop(server);

    // Client reads when server is gone
    let len = client.read(&mut buf).await.unwrap();
    assert_eq!(len, 0);

    drop(client);

    let server = NamedPipeServerBuilder::new(PIPE_NAME).build()?.accept();
    let client = NamedPipe::connect(PIPE_NAME);
    let (mut server, mut client) = try_join(server, client).await?;

    ping_pong(&mut server, &mut client).await?;

    drop(client);

    // Server reads when client is gone
    let len = server.read(&mut buf).await?;
    assert_eq!(len, 0);

    // There is no way to connect to a connected server instance
    // even if client is gone.
    let timeout = sleep(Duration::from_millis(300));
    let client = NamedPipe::connect(PIPE_NAME);
    futures::pin_mut!(client);
    futures::pin_mut!(timeout);
    let result = select(timeout, client).await;
    assert!(matches!(result, Either::Left(_)));

    Ok(())
}

async fn ping_pong(l: &mut NamedPipe, r: &mut NamedPipe) -> io::Result<()> {
    let mut buf = [b' '; 5];

    l.write_all(b"ping").await?;
    r.read(&mut buf).await?;
    assert_eq!(&buf, b"ping ");
    r.write_all(b"pong").await?;
    l.read(&mut buf).await?;
    assert_eq!(&buf, b"pong ");
    Ok(())
}

#[tokio::test]
async fn immediate_disconnect() -> io::Result<()> {
    const PIPE_NAME: &'static str = r"\\.\pipe\test-named-pipe-immediate-disconnect";

    let server = NamedPipeServerBuilder::new(PIPE_NAME).build()?;

    // there is one instance
    assert_eq!(num_instances("test-named-pipe-immediate-disconnect")?, 1);

    let _ = NamedPipe::connect(PIPE_NAME).await?;

    let mut instance = server.accept().await?;

    // instance will be broken because client is gone
    match instance.write_all(b"ping").await {
        Err(e) if e.raw_os_error() == Some(ERROR_NO_DATA as i32) => (),
        x => panic!("{:?}", x),
    }

    Ok(())
}

#[tokio::test]
async fn connection_order() -> io::Result<()> {
    const PIPE_NAME: &'static str = r"\\.\pipe\test-named-pipe-connection-order";

    let server = NamedPipeServerBuilder::new(PIPE_NAME).build()?;

    // Clients must connect to instances in a natural order, or this loop will hang
    for _ in 0..1024 {
        // Some time to finally close last loop's handles.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        let servers = vec![server.accept(), server.accept(), server.accept()];

        // now there are four instances
        assert_eq!(num_instances("test-named-pipe-connection-order")?, 4);

        let clients = vec![
            NamedPipe::connect(PIPE_NAME),
            NamedPipe::connect(PIPE_NAME),
            NamedPipe::connect(PIPE_NAME),
        ];

        let (mut servers, mut clients) =
            try_join(try_join_all(servers), try_join_all(clients)).await?;

        for s in servers.iter_mut() {
            s.write_all(b"ping").await?;
        }

        for c in clients[..3].iter_mut() {
            let mut buf = [0_u8; 4];
            c.read(&mut buf).await?;
            assert_eq!(&buf, b"ping");
            c.write_all(b"pong").await?;
        }

        for s in servers.iter_mut() {
            let mut buf = [0_u8; 4];
            s.read(&mut buf).await?;
            assert_eq!(&buf, b"pong");
        }
    }

    Ok(())
}

fn num_instances<T: AsRef<str>>(pipe_name: T) -> io::Result<u32> {
    let mut name = pipe_name.as_ref().encode_utf16().collect::<Vec<_>>();
    let mut name = UNICODE_STRING {
        Length: (name.len() * size_of::<u16>()) as u16,
        MaximumLength: (name.len() * size_of::<u16>()) as u16,
        Buffer: name.as_mut_ptr(),
    };
    let root = std::fs::File::open(r"\\.\Pipe\")?;
    let mut io_status_block = unsafe { zeroed() };
    let mut file_directory_information = [0_u8; 1024];

    let status = unsafe {
        NtQueryDirectoryFile(
            root.as_raw_handle(),
            std::ptr::null_mut(),
            None,
            std::ptr::null_mut(),
            &mut io_status_block,
            &mut file_directory_information as *mut _ as *mut c_void,
            1024,
            FileDirectoryInformation,
            0,
            &mut name,
            0,
        )
    };

    if status as u32 != NO_ERROR {
        return Err(io::Error::last_os_error());
    }

    let info = unsafe { transmute::<_, &FILE_DIRECTORY_INFORMATION>(&file_directory_information) };
    let raw_name = unsafe {
        from_raw_parts(
            info.FileName.as_ptr(),
            info.FileNameLength as usize / size_of::<u16>(),
        )
    };
    let name = String::from_utf16(raw_name).unwrap();
    let num_instances = unsafe { *info.EndOfFile.QuadPart() };

    assert_eq!(name, pipe_name.as_ref());

    Ok(num_instances as u32)
}
