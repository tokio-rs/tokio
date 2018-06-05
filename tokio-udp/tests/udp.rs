extern crate futures;
extern crate tokio_udp;
extern crate tokio_codec;
#[macro_use]
extern crate tokio_io;
extern crate bytes;
extern crate env_logger;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll, Stream, Sink};

use tokio_udp::{UdpSocket, UdpFramed};
use tokio_codec::{Encoder, Decoder};
use bytes::{BytesMut, BufMut};

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

fn send_messages<S: SendFn + Clone, R: RecvFn + Clone>(send: S, recv: R) {
    let mut a = t!(UdpSocket::bind(&([127, 0, 0, 1], 0).into()));
    let mut b = t!(UdpSocket::bind(&([127, 0, 0, 1], 0).into()));
    let a_addr = t!(a.local_addr());
    let b_addr = t!(b.local_addr());

    {
        let send = SendMessage::new(a, send.clone(), b_addr, b"1234");
        let recv = RecvMessage::new(b, recv.clone(), a_addr, b"1234");
        let (sendt, received) = t!(send.join(recv).wait());
        a = sendt;
        b = received;
    }

    {
        let send = SendMessage::new(a, send, b_addr, b"");
        let recv = RecvMessage::new(b, recv, a_addr, b"");
        t!(send.join(recv).wait());
    }
}

#[test]
fn send_to_and_recv_from() {
   send_messages(SendTo {}, RecvFrom {});
}

#[test]
fn send_and_recv() {
    send_messages(Send {}, Recv {});
}

trait SendFn {
    fn send(&self, &mut UdpSocket, &[u8], &SocketAddr) -> Result<usize, io::Error>;
}

#[derive(Debug, Clone)]
struct SendTo {}

impl SendFn for SendTo {
    fn send(&self, socket: &mut UdpSocket, buf: &[u8], addr: &SocketAddr) -> Result<usize, io::Error> {
        socket.send_to(buf, addr)
    }
}

#[derive(Debug, Clone)]
struct Send {}

impl SendFn for Send {
    fn send(&self, socket: &mut UdpSocket, buf: &[u8], addr: &SocketAddr) -> Result<usize, io::Error> {
        socket.connect(addr).expect("could not connect");
        socket.send(buf)
    }
}

struct SendMessage<S> {
    socket: Option<UdpSocket>,
    send: S,
    addr: SocketAddr,
    data: &'static [u8],
}

impl<S: SendFn> SendMessage<S> {
    fn new(socket: UdpSocket, send: S, addr: SocketAddr, data: &'static [u8]) -> SendMessage<S> {
        SendMessage {
            socket: Some(socket),
            send: send,
            addr: addr,
            data: data,
        }
    }
}

impl<S: SendFn> Future for SendMessage<S> {
    type Item = UdpSocket;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<UdpSocket, io::Error> {
        let n = try_nb!(self.send.send(self.socket.as_mut().unwrap(), &self.data[..], &self.addr));

        assert_eq!(n, self.data.len());

        Ok(self.socket.take().unwrap().into())
    }
}

trait RecvFn {
    fn recv(&self, &mut UdpSocket, &mut [u8], &SocketAddr) -> Result<usize, io::Error>;
}

#[derive(Debug, Clone)]
struct RecvFrom {}

impl RecvFn for RecvFrom {
    fn recv(&self, socket: &mut UdpSocket, buf: &mut [u8],
            expected_addr: &SocketAddr) -> Result<usize, io::Error> {
        socket.recv_from(buf).map(|(s, addr)| {
            assert_eq!(addr, *expected_addr);
            s
        })
    }
}

#[derive(Debug, Clone)]
struct Recv {}

impl RecvFn for Recv {
    fn recv(&self, socket: &mut UdpSocket, buf: &mut [u8], _: &SocketAddr) -> Result<usize, io::Error> {
        socket.recv(buf)
    }
}

struct RecvMessage<R> {
    socket: Option<UdpSocket>,
    recv: R,
    expected_addr: SocketAddr,
    expected_data: &'static [u8],
}

impl<R: RecvFn> RecvMessage<R> {
    fn new(socket: UdpSocket, recv: R, expected_addr: SocketAddr,
           expected_data: &'static [u8]) -> RecvMessage<R> {
        RecvMessage {
            socket: Some(socket),
            recv: recv,
            expected_addr: expected_addr,
            expected_data: expected_data,
        }
    }
}

impl<R: RecvFn> Future for RecvMessage<R> {
    type Item = UdpSocket;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<UdpSocket, io::Error> {
        let mut buf = vec![0u8; 10 + self.expected_data.len() * 10];
        let n = try_nb!(self.recv.recv(&mut self.socket.as_mut().unwrap(), &mut buf[..],
                                       &self.expected_addr));

        assert_eq!(n, self.expected_data.len());
        assert_eq!(&buf[..self.expected_data.len()], &self.expected_data[..]);

        Ok(self.socket.take().unwrap().into())
    }
}

#[test]
fn send_dgrams() {
    let mut a = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse())));
    let mut b = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse())));
    let mut buf = [0u8; 50];
    let b_addr = t!(b.local_addr());

    {
        let send = a.send_dgram(&b"4321"[..], &b_addr);
        let recv = b.recv_dgram(&mut buf[..]);
        let (sendt, received) = t!(send.join(recv).wait());
        assert_eq!(received.2, 4);
        assert_eq!(&received.1[..4], b"4321");
        a = sendt.0;
        b = received.0;
    }

    {
        let send = a.send_dgram(&b""[..], &b_addr);
        let recv = b.recv_dgram(&mut buf[..]);
        let received = t!(send.join(recv).wait()).1;
        assert_eq!(received.2, 0);
    }
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

#[test]
fn send_framed() {
    drop(env_logger::init());

    let mut a_soc = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse())));
    let mut b_soc = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse())));
    let a_addr = t!(a_soc.local_addr());
    let b_addr = t!(b_soc.local_addr());

    {
        let a = UdpFramed::new(a_soc, ByteCodec);
        let b = UdpFramed::new(b_soc, ByteCodec);

        let msg = b"4567".to_vec();

        let send = a.send((msg.clone(), b_addr));
        let recv = b.into_future().map_err(|e| e.0);
        let (sendt, received) = t!(send.join(recv).wait());

        let (data, addr) = received.0.unwrap();
        assert_eq!(msg, data);
        assert_eq!(a_addr, addr);

        a_soc = sendt.into_inner();
        b_soc = received.1.into_inner();
    }

    {
        let a = UdpFramed::new(a_soc, ByteCodec);
        let b = UdpFramed::new(b_soc, ByteCodec);

        let msg = b"".to_vec();

        let send = a.send((msg.clone(), b_addr));
        let recv = b.into_future().map_err(|e| e.0);
        let received = t!(send.join(recv).wait()).1;

        let (data, addr) = received.0.unwrap();
        assert_eq!(msg, data);
        assert_eq!(a_addr, addr);
    }
}
