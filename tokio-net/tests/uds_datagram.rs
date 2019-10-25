#![cfg(unix)]
#![warn(rust_2018_idioms)]

use tokio_net::uds::*;

use std::io;
use tempfile;

// struct StringDatagramCodec;

// /// A codec to decode datagrams from a unix domain socket as utf-8 text messages.
// impl Encoder for StringDatagramCodec {
//     type Item = String;
//     type Error = io::Error;

//     fn encode(&mut self, item: Self::Item, dst: &mut BytesMut) -> Result<(), Self::Error> {
//         dst.extend_from_slice(&item.into_bytes());
//         Ok(())
//     }
// }

// /// A codec to decode datagrams from a unix domain socket as utf-8 text messages.
// impl Decoder for StringDatagramCodec {
//     type Item = String;
//     type Error = io::Error;

//     fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
//         let decoded = str::from_utf8(buf)
//             .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
//             .to_string();

//         Ok(Some(decoded))
//     }
// }

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
async fn try_clone_duplicates_socket() -> io::Result<()> {
    let (mut dgram1, mut dgram2) = UnixDatagram::pair()?;
    let payload = b"ok";
    let mut buf = vec![0u8; 8];

    dgram1.send(payload).await?;
    dgram1.send(payload).await?;

    let mut dgram3 = dgram2.try_clone()?;

    let len = dgram2.recv(&mut buf).await?;
    assert_eq!(&buf[0..len], payload);
    let len = dgram3.recv(&mut buf).await?;
    assert_eq!(&buf[0..len], payload);

    dgram2.send(payload).await?;
    dgram3.send(payload).await?;

    let len = dgram1.recv(&mut buf).await?;
    assert_eq!(&buf[0..len], payload);
    let len = dgram1.recv(&mut buf).await?;
    assert_eq!(&buf[0..len], payload);

    Ok(())
}
