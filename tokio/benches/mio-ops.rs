// Measure cost of different operations
// to get a sense of performance tradeoffs
#![warn(rust_2018_idioms)]

use criterion::{criterion_group, criterion_main, Bencher, Criterion};

use mio::tcp::TcpListener;
use mio::{PollOpt, Ready, Token};

fn mio_register_deregister(b: &mut Bencher<'_>) {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Setup the server socket
    let sock = TcpListener::bind(&addr).unwrap();
    let poll = mio::Poll::new().unwrap();

    const CLIENT: Token = Token(1);

    b.iter(|| {
        poll.register(&sock, CLIENT, Ready::readable(), PollOpt::edge())
            .unwrap();
        poll.deregister(&sock).unwrap();
    });
}

fn mio_reregister(b: &mut Bencher<'_>) {
    let addr = "127.0.0.1:0".parse().unwrap();
    // Setup the server socket
    let sock = TcpListener::bind(&addr).unwrap();
    let poll = mio::Poll::new().unwrap();

    const CLIENT: Token = Token(1);
    poll.register(&sock, CLIENT, Ready::readable(), PollOpt::edge())
        .unwrap();

    b.iter(|| {
        poll.reregister(&sock, CLIENT, Ready::readable(), PollOpt::edge())
            .unwrap();
    });
    poll.deregister(&sock).unwrap();
}

fn mio_poll(b: &mut Bencher<'_>) {
    let poll = mio::Poll::new().unwrap();
    let timeout = std::time::Duration::new(0, 0);
    let mut events = mio::Events::with_capacity(1024);

    b.iter(|| {
        poll.poll(&mut events, Some(timeout)).unwrap();
    });
}

fn bench_mio_ops(c: &mut Criterion) {
    c.bench_function("mio_register_deregister", mio_register_deregister);
    c.bench_function("mio_reregister", mio_reregister);
    c.bench_function("mio_poll", mio_poll);
}

criterion_group!(mio_ops, bench_mio_ops);
criterion_main!(mio_ops);
