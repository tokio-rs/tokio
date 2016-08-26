extern crate futures;
extern crate tokio_core;

use std::io;
use std::net::SocketAddr;

use futures::{Future, Poll};
use tokio_core::UdpSocket;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn send_messages() {
    let mut l = t!(tokio_core::Loop::new());
    let a = l.handle().udp_bind(&"127.0.0.1:0".parse().unwrap());
    let b = l.handle().udp_bind(&"127.0.0.1:0".parse().unwrap());
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
        match self.socket.send_to(b"1234", &self.addr) {
            Ok(4) => Poll::Ok(()),
            Ok(n) => panic!("didn't send 4 bytes: {}", n),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::NotReady,
            Err(e) => Poll::Err(e),
        }
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
        match self.socket.recv_from(&mut buf) {
            Ok((4, addr)) => {
                assert_eq!(&buf[..4], b"1234");
                assert_eq!(addr, self.expected_addr);
                Poll::Ok(())
            }
            Ok((n, _)) => panic!("didn't read 4 bytes: {}", n),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Poll::NotReady,
            Err(e) => Poll::Err(e),
        }
    }
}
