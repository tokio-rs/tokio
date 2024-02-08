// This example shows how to use the tokio runtime with any other executor
//
//It takes advantage from RuntimeExt which provides the extension to customize your
//runtime.

use tokio::net::TcpListener;
use tokio::runtime::Builder;
use tokio::sync::oneshot;
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
    rt1.block_on(rt2.wrap(async move {
        let listener = TcpListener::bind("0.0.0.0:0").await.unwrap();
        println!("addr: {:?}", listener.local_addr());
        tx.send(()).unwrap();
    }));
    futures::executor::block_on(rx).unwrap();
}
