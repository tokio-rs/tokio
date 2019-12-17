#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::net::UdpSocket;

const MSG: &[u8] = b"hello";
const MSG_LEN: usize = MSG.len();

#[tokio::test]
async fn send_recv() -> std::io::Result<()> {
    let mut sender = UdpSocket::bind("127.0.0.1:0").await?;
    let mut receiver = UdpSocket::bind("127.0.0.1:0").await?;

    sender.connect(receiver.local_addr()?).await?;
    receiver.connect(sender.local_addr()?).await?;

    sender.send(MSG).await?;

    let mut recv_buf = [0u8; 32];
    let len = receiver.recv(&mut recv_buf[..]).await?;

    assert_eq!(&recv_buf[..len], MSG);
    Ok(())
}

#[tokio::test]
async fn send_to_recv_from() -> std::io::Result<()> {
    let mut sender = UdpSocket::bind("127.0.0.1:0").await?;
    let mut receiver = UdpSocket::bind("127.0.0.1:0").await?;

    let receiver_addr = receiver.local_addr()?;
    sender.send_to(MSG, &receiver_addr).await?;

    let mut recv_buf = [0u8; 32];
    let (len, addr) = receiver.recv_from(&mut recv_buf[..]).await?;

    assert_eq!(&recv_buf[..len], MSG);
    assert_eq!(addr, sender.local_addr()?);
    Ok(())
}

#[tokio::test]
async fn split() -> std::io::Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:0").await?;
    let (mut r, mut s) = socket.split();

    let addr = s.as_ref().local_addr()?;
    tokio::spawn(async move {
        s.send_to(MSG, &addr).await.unwrap();
    });
    let mut recv_buf = [0u8; 32];
    let (len, _) = r.recv_from(&mut recv_buf[..]).await?;
    assert_eq!(&recv_buf[..len], MSG);
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

#[tokio::test]
async fn try_send_spawn() -> std::io::Result<()> {
    const MSG2: &[u8] = b"world!";
    const MSG2_LEN: usize = MSG2.len();

    let sender = UdpSocket::bind("127.0.0.1:0").await?;
    let mut receiver = UdpSocket::bind("127.0.0.1:0").await?;

    receiver.connect(sender.local_addr()?).await?;

    let sent = &sender.try_send_to(MSG, &receiver.local_addr()?).await?;
    assert_eq!(sent, &MSG_LEN);

    sender.connect(receiver.local_addr()?).await?;
    let sent = &sender.try_send(MSG2)?;
    assert_eq!(sent, &MSG2_LEN);

    std::thread::spawn(move || {
        let sent = &sender.try_send(MSG).unwrap();
        assert_eq!(sent, &MSG_LEN);
    })
    .join()
    .unwrap();

    let mut buf = [0u8; 32];
    let mut received = receiver.recv(&mut buf[..]).await?;
    received += receiver.recv(&mut buf[..]).await?;
    received += receiver.recv(&mut buf[..]).await?;
    assert_eq!(received, MSG_LEN * 2 + MSG2_LEN);
    Ok(())
}
