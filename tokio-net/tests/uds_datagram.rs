#![cfg(unix)]
#![feature(async_await)]
#![warn(rust_2018_idioms)]

use futures_util::{future::FutureExt, sink::SinkExt, stream::StreamExt, try_future::try_join};
use tokio_codec::DatagramFramed;
use tokio_net::uds::*;

use std::io;
use tempfile;

mod support;
use support::ByteCodec;

async fn echo_server(mut socket: UnixDatagram) -> io::Result<()> {
    let mut recv_buf = vec![0u8; 1024];
    loop {
        let (len, peer_addr) = socket.recv_from(&mut recv_buf[..]).await?;
        if let Some(path) = peer_addr.as_pathname() {
            socket.send_to(&recv_buf[..len], path).await?;
        }
    }
}

#[tokio::test]
async fn echo() -> io::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let server_path = dir.path().join("server.sock");
    let client_path = dir.path().join("client.sock");

    let server_socket = UnixDatagram::bind(server_path.clone())?;

    tokio::spawn(async move {
        if let Err(e) = echo_server(server_socket).await {
            eprintln!("Error in echo server: {}", e);
        }
    });

    {
        let mut socket = UnixDatagram::bind(&client_path).unwrap();
        socket.connect(server_path)?;
        socket.send(b"ECHO").await?;
        let mut recv_buf = [0u8; 16];
        let len = socket.recv(&mut recv_buf[..]).await?;
        assert_eq!(&recv_buf[..len], b"ECHO");
    }

    Ok(())
}

#[tokio::test]
async fn send_framed() -> std::io::Result<()> {
    let dir = tempfile::tempdir().unwrap();
    let a_path = dir.path().join("a.sock");
    let b_path = dir.path().join("b.sock");

    let mut a_soc = UnixDatagram::bind(a_path.clone())?;
    let mut b_soc = UnixDatagram::bind(b_path.clone())?;

    let a_addr = a_soc.local_addr()?;
    let b_addr = b_soc.local_addr()?;

    // test sending & receiving bytes
    {
        let mut a = DatagramFramed::new(a_soc, ByteCodec);
        let mut b = DatagramFramed::new(b_soc, ByteCodec);

        let msg = b"4567".to_vec();

        let send = a.send((msg.clone(), b_addr.clone()));
        let recv = b.next().map(|e| e.unwrap());
        let (_, received) = try_join(send, recv).await.unwrap();

        let (data, addr) = received;
        assert_eq!(msg, data);
        assert_eq!(a_addr.as_pathname(), addr.as_pathname());

        a_soc = a.into_inner();
        b_soc = b.into_inner();
    }

    // test sending & receiving an empty message
    {
        let mut a = DatagramFramed::new(a_soc, ByteCodec);
        let mut b = DatagramFramed::new(b_soc, ByteCodec);

        let msg = b"".to_vec();

        let send = a.send((msg.clone(), b_addr.clone()));
        let recv = b.next().map(|e| e.unwrap());
        let (_, received) = try_join(send, recv).await.unwrap();

        let (data, addr) = received;
        assert_eq!(msg, data);
        assert_eq!(a_addr.as_pathname(), addr.as_pathname());
    }

    Ok(())
}
