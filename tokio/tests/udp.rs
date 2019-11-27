#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::net::UdpSocket;

#[tokio::test]
async fn send_recv() -> std::io::Result<()> {
    let mut sender = UdpSocket::bind("127.0.0.1:0").await?;
    let mut receiver = UdpSocket::bind("127.0.0.1:0").await?;

    sender.connect(receiver.local_addr()?).await?;
    receiver.connect(sender.local_addr()?).await?;

    let message = b"hello!";
    sender.send(message).await?;

    let mut recv_buf = [0u8; 32];
    let len = receiver.recv(&mut recv_buf[..]).await?;

    assert_eq!(&recv_buf[..len], message);
    Ok(())
}

#[tokio::test]
async fn send_to_recv_from() -> std::io::Result<()> {
    let mut sender = UdpSocket::bind("127.0.0.1:0").await?;
    let mut receiver = UdpSocket::bind("127.0.0.1:0").await?;

    let message = b"hello!";
    let receiver_addr = receiver.local_addr()?;
    sender.send_to(message, &receiver_addr).await?;

    let mut recv_buf = [0u8; 32];
    let (len, addr) = receiver.recv_from(&mut recv_buf[..]).await?;

    assert_eq!(&recv_buf[..len], message);
    assert_eq!(addr, sender.local_addr()?);
    Ok(())
}

#[tokio::test]
async fn split() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let (mut r, mut s) = socket.split();

    let msg = b"hello";
    let addr = s.as_ref().local_addr()?;
    tokio::spawn(async move {
        s.send_to(msg, &addr).await.unwrap();
    });
    let mut recv_buf = [0u8; 32];
    let (len, _) = r.recv_from(&mut recv_buf[..]).await?;
    assert_eq!(&recv_buf[..len], msg);
    Ok(())
}

#[tokio::test]
async fn reunite() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let (s, r) = socket.split();
    assert!(s.reunite(r).is_ok());
    Ok(())
}

#[tokio::test]
async fn reunite_error() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let socket1 = UdpSocket::bind("127.0.0.1:0").await?;
    let (s, _) = socket.split();
    let (_, r1) = socket1.split();
    assert!(s.reunite(r1).is_err());
    Ok(())
}
