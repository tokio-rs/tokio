#![warn(rust_2018_idioms)]
#![cfg(all(feature = "full", not(target_os = "wasi"), not(miri)))]

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

#[test]
#[should_panic(expected = "max_io_events_per_busy_tick must be non-zero")]
fn zero_busy_tick_panics() {
    tokio::runtime::Builder::new_multi_thread().max_io_events_per_busy_tick(0);
}

#[test]
fn busy_workers_still_get_every_event() {
    // Every worker always has a task, so it polls the driver only at its
    // maintenance tick, and each poll takes one event. The events each poll
    // leaves in the kernel must still reach their tasks.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_io_events_per_busy_tick(1)
        .enable_all()
        .build()
        .unwrap();
    for _ in 0..8 {
        rt.spawn(async {
            loop {
                tokio::task::yield_now().await;
            }
        });
    }
    rt.block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let mut buf = [0u8; 4];
                    while s.read_exact(&mut buf).await.is_ok() {
                        s.write_all(&buf).await.unwrap();
                    }
                });
            }
        });
        let clients: Vec<_> = (0..16)
            .map(|_| {
                tokio::spawn(async move {
                    let mut s = TcpStream::connect(addr).await.unwrap();
                    for _ in 0..16 {
                        s.write_all(b"ping").await.unwrap();
                        s.read_exact(&mut [0u8; 4]).await.unwrap();
                    }
                })
            })
            .collect();
        let all = async {
            for c in clients {
                c.await.unwrap();
            }
        };
        tokio::time::timeout(Duration::from_secs(20), all)
            .await
            .expect("echo round trips did not finish");
    });
}
