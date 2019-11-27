use tokio::net::UdpSocket;
use tokio_util::codec::{Decoder, Encoder};
use tokio_util::udp::UdpFramed;

use bytes::{BufMut, BytesMut};
use futures::future::try_join;
use futures::future::FutureExt;
use futures::sink::SinkExt;
use futures::stream::StreamExt;
use std::io;

#[tokio::test]
async fn send_framed() -> std::io::Result<()> {
    let mut a_soc = UdpSocket::bind("127.0.0.1:0").await?;
    let mut b_soc = UdpSocket::bind("127.0.0.1:0").await?;

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
        buf.put_slice(&data);
        Ok(())
    }
}
