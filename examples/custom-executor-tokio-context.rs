// This example shows how to use the tokio runtime with any other executor
//
// The main components are a spawn fn that will wrap futures in a special future
// that will always enter the tokio context on poll. This only spawns on extra thread
// to manage and run the tokio drivers in the background.

use tokio::net::TcpListener;
use tokio::runtime::Builder;
use tokio::sync::oneshot;
use tokio::time::{Duration, Sleep};
use tokio_util::context::RuntimeExt;

fn main() {
    let (tx, rx) = oneshot::channel();
    let rt1 = Builder::new_multi_thread()
        .worker_threads(1)
        // no timer!
        .build()
        .unwrap();
    let rt2 = Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .unwrap();

    // Without the `HandleExt.wrap()` there would be a panic because there is
    // no timer running, since it would be referencing runtime r1.
    let _ = rt1.block_on(rt2.wrap(async move {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        println!("addr: {:?}", listener.local_addr());
        tx.send(()).unwrap();
    }));
}
