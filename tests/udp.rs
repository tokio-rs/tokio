extern crate futures;
extern crate tokio;
#[macro_use]
extern crate tokio_io;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll, Stream, Sink};
use tokio::net::{UdpSocket, UdpCodec};

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
    fn send(&self, &UdpSocket, &[u8], &SocketAddr) -> Result<usize, io::Error>;
}

#[derive(Debug, Clone)]
struct SendTo {}

impl SendFn for SendTo {
    fn send(&self, socket: &UdpSocket, buf: &[u8], addr: &SocketAddr) -> Result<usize, io::Error> {
        socket.send_to(buf, addr)
    }
}

#[derive(Debug, Clone)]
struct Send {}

impl SendFn for Send {
    fn send(&self, socket: &UdpSocket, buf: &[u8], addr: &SocketAddr) -> Result<usize, io::Error> {
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
        let n = try_nb!(self.send.send(self.socket.as_ref().unwrap(), &self.data[..], &self.addr));

        assert_eq!(n, self.data.len());

        Ok(self.socket.take().unwrap().into())
    }
}

trait RecvFn {
    fn recv(&self, &UdpSocket, &mut [u8], &SocketAddr) -> Result<usize, io::Error>;
}

#[derive(Debug, Clone)]
struct RecvFrom {}

impl RecvFn for RecvFrom {
    fn recv(&self, socket: &UdpSocket, buf: &mut [u8],
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
    fn recv(&self, socket: &UdpSocket, buf: &mut [u8], _: &SocketAddr) -> Result<usize, io::Error> {
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
        let n = try_nb!(self.recv.recv(&self.socket.as_ref().unwrap(), &mut buf[..],
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
        let send = a.send_dgram(&b"4321"[..], b_addr);
        let recv = b.recv_dgram(&mut buf[..]);
        let (sendt, received) = t!(send.join(recv).wait());
        assert_eq!(received.2, 4);
        assert_eq!(&received.1[..4], b"4321");
        a = sendt.0;
        b = received.0;
    }

    {
        let send = a.send_dgram(&b""[..], b_addr);
        let recv = b.recv_dgram(&mut buf[..]);
        let received = t!(send.join(recv).wait()).1;
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
    let mut a_soc = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse())));
    let mut b_soc = t!(UdpSocket::bind(&t!("127.0.0.1:0".parse())));
    let a_addr = t!(a_soc.local_addr());
    let b_addr = t!(b_soc.local_addr());

    {
        let a = a_soc.framed(Codec { data: &b"4567"[..], from: a_addr, to: b_addr});
        let b = b_soc.framed(Codec { data: &b"4567"[..], from: a_addr, to: b_addr});

        let send = a.send(&b"4567"[..]);
        let recv = b.into_future().map_err(|e| e.0);
        let (sendt, received) = t!(send.join(recv).wait());
        assert_eq!(received.0, Some(()));

        a_soc = sendt.into_inner();
        b_soc = received.1.into_inner();
    }

    {
        let a = a_soc.framed(Codec { data: &b""[..], from: a_addr, to: b_addr});
        let b = b_soc.framed(Codec { data: &b""[..], from: a_addr, to: b_addr});

        let send = a.send(&b""[..]);
        let recv = b.into_future().map_err(|e| e.0);
        let received = t!(send.join(recv).wait()).1;
        assert_eq!(received.0, Some(()));
    }
}

