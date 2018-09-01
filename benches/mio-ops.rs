// Measure cost of different operations
// to get a sense of performance tradeoffs
#![feature(test)]
#![deny(warnings)]

extern crate test;
extern crate mio;

use test::Bencher;

use mio::tcp::TcpListener;
use mio::{Token, Ready, PollOpt};


#[bench]
fn mio_register_deregister(b: &mut Bencher) {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Setup the server socket
    let sock = TcpListener::bind(&addr).unwrap();
    let poll = mio::Poll::new().unwrap();

    const CLIENT: Token = Token(1);

    b.iter(|| {
        poll.register(&sock, CLIENT, Ready::readable(),
              PollOpt::edge()).unwrap();
        poll.deregister(&sock).unwrap();
    });
}

#[bench]
fn mio_reregister(b: &mut Bencher) {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Setup the server socket
    let sock = TcpListener::bind(&addr).unwrap();
    let poll = mio::Poll::new().unwrap();

    const CLIENT: Token = Token(1);
    poll.register(&sock, CLIENT, Ready::readable(),
    PollOpt::edge()).unwrap();

    b.iter(|| {
        poll.reregister(&sock, CLIENT, Ready::readable(),
        PollOpt::edge()).unwrap();
    });
    poll.deregister(&sock).unwrap();
}

#[bench]
fn mio_poll(b: &mut Bencher) {
    let poll = mio::Poll::new().unwrap();
    let timeout = std::time::Duration::new(0, 0);
    let mut events = mio::Events::with_capacity(1024);

    b.iter(|| {
        poll.poll(&mut events, Some(timeout)).unwrap();
    });
}
