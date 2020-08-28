#![warn(rust_2018_idioms)]
#![cfg(feature = "sync")]

use tokio::runtime;
use tokio::sync::oneshot;

use std::sync::mpsc;
use std::thread;

#[test]
fn basic_shell_rt() {
    let (feed_tx, feed_rx) = mpsc::channel::<oneshot::Sender<()>>();

    let th = thread::spawn(move || {
        for tx in feed_rx.iter() {
            tx.send(()).unwrap();
        }
    });

    for _ in 0..1_000 {
        let rt = runtime::Builder::new().build().unwrap();

        let (tx, rx) = oneshot::channel();

        feed_tx.send(tx).unwrap();

        rt.block_on(rx).unwrap();
    }

    drop(feed_tx);
    th.join().unwrap();
}
