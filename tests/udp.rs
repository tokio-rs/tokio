extern crate futures;
#[macro_use]
extern crate tokio_core;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::net::UdpSocket;
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
    let a = UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle());
    let b = UdpSocket::bind(&t!("127.0.0.1:0".parse()), &l.handle());
    let (a, b) = t!(l.run(a.join(b)));
    let a_addr = t!(a.local_addr());
    let b_addr = t!(b.local_addr());

    let send = SendMessage { socket: a, addr: b_addr };
    let recv = RecvMessage { socket: b, expected_addr: a_addr };
    t!(l.run(send.join(recv)));
}

struct SendMessage {
    socket: UdpSocket,
    addr: SocketAddr,
}

impl Future for SendMessage {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        let n = try_nb!(self.socket.send_to(b"1234", &self.addr));
        assert_eq!(n, 4);
        Ok(().into())
    }
}

struct RecvMessage {
    socket: UdpSocket,
    expected_addr: SocketAddr,
}

impl Future for RecvMessage {
    type Item = ();
    type Error = io::Error;

    fn poll(&mut self) -> Poll<(), io::Error> {
        let mut buf = [0; 32];
        let (n, addr) = try_nb!(self.socket.recv_from(&mut buf));
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], b"1234");
        assert_eq!(addr, self.expected_addr);
        Ok(().into())
    }
}
