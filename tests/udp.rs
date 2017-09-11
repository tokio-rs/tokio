extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll, Stream, Sink};
use tokio_core::net::{UdpSocket, UdpCodec};
use tokio_core::reactor::Core;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn send_messages() {
    let mut l = t!(Core::new());
    let mut a = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle()));
    let mut b = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle()));
    let a_addr = t!(a.local_addr());
    let b_addr = t!(b.local_addr());

    {
        let send = SendMessage::new(a, b_addr, b"1234");
        let recv = RecvMessage::new(b, a_addr, b"1234");
        let (sendt, received) = t!(l.run(send.join(recv)));
        a = sendt;
        b = received;
    }

    {
        let send = SendMessage::new(a, b_addr, b"");
        let recv = RecvMessage::new(b, a_addr, b"");
        t!(l.run(send.join(recv)));
    }
}

struct SendMessage {
    socket: Option<UdpSocket>,
    addr: SocketAddr,
    data: &'static [u8],
}

impl SendMessage {
    fn new(socket: UdpSocket, addr: SocketAddr, data: &'static [u8]) -> SendMessage {
        SendMessage {
            socket: Some(socket),
            addr: addr,
            data: data,
        }
    }
}

impl Future for SendMessage {
    type Item = UdpSocket;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<UdpSocket, io::Error> {
        let n = try_nb!(self.socket.as_ref().unwrap()
            .send_to(&self.data[..], &self.addr));

        assert_eq!(n, self.data.len());

        Ok(self.socket.take().unwrap().into())
    }
}

struct RecvMessage {
    socket: Option<UdpSocket>,
    addr: SocketAddr,
    data: &'static [u8],
}

impl RecvMessage {
    fn new(socket: UdpSocket, expected_addr: SocketAddr,
           expected_data: &'static [u8]) -> RecvMessage
    {
        RecvMessage {
            socket: Some(socket),
            addr: expected_addr,
            data: expected_data,
        }
    }
}

impl Future for RecvMessage {
    type Item = UdpSocket;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<UdpSocket, io::Error> {
        let mut buf = vec![0u8; 10 + self.data.len() * 10];
        let (n, addr) = try_nb!(self.socket.as_ref().unwrap()
            .recv_from(&mut buf[..]));

        assert_eq!(n, self.data.len());
        assert_eq!(&buf[..self.data.len()], &self.data[..]);
        assert_eq!(addr, self.addr);

        Ok(self.socket.take().unwrap().into())
    }
}

#[test]
fn send_dgrams() {
    let mut l = t!(Core::new());
    let mut a = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle()));
    let mut b = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle()));
    let mut buf = [0u8; 50];
    let b_addr = t!(b.local_addr());

    {
        let send = a.send_dgram(&b"4321"[..], b_addr);
        let recv = b.recv_dgram(&mut buf[..]);
        let (sendt, received) = t!(l.run(send.join(recv)));
        assert_eq!(received.2, 4);
        assert_eq!(&received.1[..4], b"4321");
        a = sendt.0;
        b = received.0;
    }

    {
        let send = a.send_dgram(&b""[..], b_addr);
        let recv = b.recv_dgram(&mut buf[..]);
        let received = t!(l.run(send.join(recv))).1;
        assert_eq!(received.2, 0);
    }
}

#[derive(Debug, Clone)]
struct Codec {
    data: &'static [u8],
    from: SocketAddr,
    to: SocketAddr,
}

impl UdpCodec for Codec {
    type In = ();
    type Out = &'static [u8];

    fn decode(&mut self, src: &SocketAddr, buf: &[u8]) -> io::Result<Self::In> {
        assert_eq!(src, &self.from);
        assert_eq!(buf, self.data);
        Ok(())
    }

    fn encode(&mut self, msg: Self::Out, buf: &mut Vec<u8>) -> SocketAddr {
        assert_eq!(msg, self.data);
        buf.extend_from_slice(msg);
        self.to
    }
}

#[test]
fn send_framed() {
    let mut l = t!(Core::new());
    let mut a_soc = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle()));
    let mut b_soc = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle()));
    let a_addr = t!(a_soc.local_addr());
    let b_addr = t!(b_soc.local_addr());

    {
        let a = a_soc.framed(Codec { data: &b"4567"[..], from: a_addr, to: b_addr});
        let b = b_soc.framed(Codec { data: &b"4567"[..], from: a_addr, to: b_addr});

        let send = a.send(&b"4567"[..]);
        let recv = b.into_future().map_err(|e| e.0);
        let (sendt, received) = t!(l.run(send.join(recv)));
        assert_eq!(received.0, Some(()));

        a_soc = sendt.into_inner();
        b_soc = received.1.into_inner();
    }

    {
        let a = a_soc.framed(Codec { data: &b""[..], from: a_addr, to: b_addr});
        let b = b_soc.framed(Codec { data: &b""[..], from: a_addr, to: b_addr});

        let send = a.send(&b""[..]);
        let recv = b.into_future().map_err(|e| e.0);
        let received = t!(l.run(send.join(recv))).1;
        assert_eq!(received.0, Some(()));
    }
}
