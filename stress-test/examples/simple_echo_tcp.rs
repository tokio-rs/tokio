//! Simple TCP echo server to check memory leaks using Valgrind.
use std::{thread::sleep, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpSocket},
    runtime::Builder,
    sync::oneshot,
};

const TCP_ENDPOINT: &str = "127.0.0.1:8080";
const NUM_MSGS: usize = 100;
const MSG_SIZE: usize = 1024;

fn main() {
    let rt = Builder::new_multi_thread().enable_io().build().unwrap();
    let rt2 = Builder::new_multi_thread().enable_io().build().unwrap();

    rt.spawn(async {
        let listener = TcpListener::bind(TCP_ENDPOINT).await.unwrap();
        let (mut socket, _) = listener.accept().await.unwrap();
        let (mut rd, mut wr) = socket.split();
        while tokio::io::copy(&mut rd, &mut wr).await.is_ok() {}
    });

    // wait a bit so that the listener binds.
    sleep(Duration::from_millis(100));

    // create a channel to let the main thread know that all the messages were sent and received.
    let (tx, mut rx) = oneshot::channel();

    rt2.spawn(async {
        let addr = TCP_ENDPOINT.parse().unwrap();
        let socket = TcpSocket::new_v4().unwrap();
        let mut stream = socket.connect(addr).await.unwrap();

        let mut buff = [0; MSG_SIZE];
        for _ in 0..NUM_MSGS {
            let one_mega_random_bytes: Vec<u8> =
                (0..MSG_SIZE).map(|_| rand::random::<u8>()).collect();
            stream
                .write_all(one_mega_random_bytes.as_slice())
                .await
                .unwrap();
            let _ = stream.read(&mut buff).await.unwrap();
        }
        tx.send(()).unwrap();
    });

    loop {
        // check that we're done.
        match rx.try_recv() {
            Err(oneshot::error::TryRecvError::Empty) => (),
            Err(oneshot::error::TryRecvError::Closed) => panic!("channel got closed..."),
            Ok(()) => break,
        }
    }
}
