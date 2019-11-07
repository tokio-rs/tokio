#![warn(rust_2018_idioms)]

use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::thread;

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use futures::channel::mpsc;
use futures::channel::oneshot;
use futures::{
    executor::{block_on, block_on_stream},
    future,
    prelude::*,
    ready,
    task::{self, Poll},
    Future,
};
use tokio::{net::UdpSocket, runtime::Runtime};

/// UDP echo server
struct EchoServer {
    socket: UdpSocket,
    buf: Vec<u8>,
    to_send: Option<(usize, SocketAddr)>,
}

impl EchoServer {
    fn new(s: UdpSocket) -> Self {
        EchoServer {
            socket: s,
            to_send: None,
            buf: vec![0u8; 1600],
        }
    }
}

impl Future for EchoServer {
    type Output = io::Result<()>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
        let self_ = &mut *self;
        loop {
            if let Some(&(size, peer)) = self_.to_send.as_ref() {
                ready!(self_.socket.poll_send_to(cx, &self_.buf[..size], &peer))?;
                self_.to_send = None;
            }
            self_.to_send = Some(ready!(self_.socket.poll_recv_from(cx, &mut self_.buf))?);
        }
    }
}

fn udp_echo_latency(b: &mut Bencher<'_>) {
    let any_addr = "127.0.0.1:0".to_string();
    let any_addr = any_addr.parse::<SocketAddr>().unwrap();

    let (stop_c, stop_p) = oneshot::channel::<()>();
    let (tx, rx) = oneshot::channel();

    let child = thread::spawn(move || {
        Runtime::new().unwrap().block_on(async {
            let socket = tokio::net::UdpSocket::bind(&any_addr).await.unwrap();
            tx.send(socket.local_addr().unwrap()).unwrap();

            let server = EchoServer::new(socket);
            let server = future::select(server, stop_p);
            server.await;
        });
    });

    let client = std::net::UdpSocket::bind(&any_addr).unwrap();

    let server_addr = block_on(rx).unwrap();
    let mut buf = [0u8; 1000];

    // warmup phase; for some reason initial couple of
    // runs are much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..8 {
        client.send_to(&buf, &server_addr).unwrap();
        let _ = client.recv_from(&mut buf).unwrap();
    }

    b.iter(|| {
        client.send_to(&buf, &server_addr).unwrap();
        let _ = client.recv_from(&mut buf).unwrap();
    });

    stop_c.send(()).unwrap();
    child.join().unwrap();
}

fn futures_channel_latency(b: &mut Bencher<'_>) {
    let (mut in_tx, mut in_rx) = mpsc::channel(32);
    let (mut out_tx, out_rx) = mpsc::channel::<_>(32);

    let child = thread::spawn(move || block_on(out_tx.send_all(&mut in_rx)));
    let mut rx_iter = block_on_stream(out_rx);

    // warmup phase; for some reason initial couple of runs are much slower
    //
    // TODO: Describe the exact reasons; caching? branch predictor? lazy closures?
    for _ in 0..8 {
        in_tx.start_send(Ok(1usize)).unwrap();
        let _ = rx_iter.next();
    }

    b.iter(|| {
        in_tx.start_send(Ok(1usize)).unwrap();
        let _ = rx_iter.next();
    });

    drop(in_tx);
    child.join().unwrap().unwrap();
}

fn bench_latency(c: &mut Criterion) {
    c.bench_function("udp_echo_latency", udp_echo_latency);
    c.bench_function("futures_channel_latency", futures_channel_latency);
}

criterion_group!(latency, bench_latency);
criterion_main!(latency);
