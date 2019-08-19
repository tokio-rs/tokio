#![feature(async_await)]
#![warn(rust_2018_idioms)]

use tokio_codec::{Decoder, Encoder};
use tokio_net::udp::{UdpFramed, UdpSocket};

use bytes::{BufMut, BytesMut};
use futures_util::{future::FutureExt, sink::SinkExt, stream::StreamExt, try_future::try_join};
use std::io;

#[tokio::test]
async fn send_recv() -> std::io::Result<()> {
    let mut sender = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
    let mut receiver = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;

    sender.connect(&receiver.local_addr()?)?;
    receiver.connect(&sender.local_addr()?)?;

    let message = b"hello!";
    sender.send(message).await?;

    let mut recv_buf = [0u8; 32];
    let len = receiver.recv(&mut recv_buf[..]).await?;

    assert_eq!(&recv_buf[..len], message);
    Ok(())
}

#[tokio::test]
async fn send_to_recv_from() -> std::io::Result<()> {
    let mut sender = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
    let mut receiver = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;

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
    let socket = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
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
    let socket = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
    let (s, r) = socket.split();
    assert!(s.reunite(r).is_ok());
    Ok(())
}

#[tokio::test]
async fn reunite_error() -> std::io::Result<()> {
    let socket = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
    let socket1 = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
    let (s, _) = socket.split();
    let (_, r1) = socket1.split();
    assert!(s.reunite(r1).is_err());
    Ok(())
}

pub struct ByteCodec;

impl Decoder for ByteCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Vec<u8>>, io::Error> {
        let len = buf.len();
        Ok(Some(buf.split_to(len).to_vec()))
    }
}

impl Encoder for ByteCodec {
    type Item = Vec<u8>;
    type Error = io::Error;

    fn encode(&mut self, data: Vec<u8>, buf: &mut BytesMut) -> Result<(), io::Error> {
        buf.reserve(data.len());
        buf.put(data);
        Ok(())
    }
}

#[tokio::test]
async fn send_framed() -> std::io::Result<()> {
    let mut a_soc = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;
    let mut b_soc = UdpSocket::bind(&"127.0.0.1:0".parse().unwrap())?;

    let a_addr = a_soc.local_addr()?;
    let b_addr = b_soc.local_addr()?;

    // test sending & receiving bytes
    {
        let mut a = UdpFramed::new(a_soc, ByteCodec);
        let mut b = UdpFramed::new(b_soc, ByteCodec);

        let msg = b"4567".to_vec();

        let send = a.send((msg.clone(), b_addr));
        let recv = b.next().map(|e| e.unwrap());
        let (_, received) = try_join(send, recv).await.unwrap();

        let (data, addr) = received;
        assert_eq!(msg, data);
        assert_eq!(a_addr, addr);

        a_soc = a.into_inner();
        b_soc = b.into_inner();
    }

    // test sending & receiving an empty message
    {
        let mut a = UdpFramed::new(a_soc, ByteCodec);
        let mut b = UdpFramed::new(b_soc, ByteCodec);

        let msg = b"".to_vec();

        let send = a.send((msg.clone(), b_addr));
        let recv = b.next().map(|e| e.unwrap());
        let (_, received) = try_join(send, recv).await.unwrap();

        let (data, addr) = received;
        assert_eq!(msg, data);
        assert_eq!(a_addr, addr);
    }

    Ok(())
}
