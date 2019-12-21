#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::runtime::Runtime;
use tokio::sync::oneshot;
use tokio::task::{self, LocalSet};

#[test]
fn acquire_mutex_in_drop() {
    use futures::future::pending;

    let (tx1, rx1) = oneshot::channel();
    let (tx2, rx2) = oneshot::channel();

    let mut rt = rt();
    let local = LocalSet::new();

    local.spawn_local(async move {
        let _ = rx2.await;
        unreachable!();
    });

    local.spawn_local(async move {
        let _ = rx1.await;
        tx2.send(()).unwrap();
        unreachable!();
    });

    // Spawn a task that will never notify
    local.spawn_local(async move {
        pending::<()>().await;
        tx1.send(()).unwrap();
    });

    // Tick the loop
    local.block_on(&mut rt, async {
        task::yield_now().await;
    });

    // Drop the LocalSet
    drop(local);
}

fn rt() -> Runtime {
    tokio::runtime::Builder::new()
        .basic_scheduler()
        .enable_all()
        .build()
        .unwrap()
}
