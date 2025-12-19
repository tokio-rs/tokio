#![cfg(all(
    target_os = "linux",
    feature = "net",
    feature = "rt",
    feature = "sync",
    feature = "macros",
    feature = "time",
    tokio_unstable,
))]

use std::sync::Arc;
use std::thread;
use tokio::sync::Barrier;

const NUM_WORKERS: usize = 8;
const NUM_CONNECTIONS: u64 = 32;

const FUDGE_MIN: f64 = 0.75;
const FUDGE_MAX: f64 = 1.25;

#[test]
fn epoll_exclusive() {
    let value = count_accepts_with_flags(NUM_WORKERS, NUM_CONNECTIONS, libc::EPOLLEXCLUSIVE as u32);

    let actual_to_expected_ratio = value as f64 / NUM_CONNECTIONS as f64;

    assert!(
        actual_to_expected_ratio >= FUDGE_MIN && actual_to_expected_ratio <= FUDGE_MAX,
        "expected fuzzy {}, got {}",
        NUM_CONNECTIONS,
        value
    );
}

fn count_accepts_with_flags(workers: usize, connections: u64, flags: u32) -> u64 {
    let barrier = Arc::new(Barrier::new(workers as usize + 1));

    let mut handles = Vec::with_capacity(workers);

    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();

    let listener_addr = listener.local_addr().unwrap();

    for _ in 0..workers {
        let local_listener = listener.try_clone().unwrap();
        let local_barrier = barrier.clone();

        handles.push(thread::spawn(move || {
            count_accepts(local_listener, flags | libc::EPOLLIN as u32, local_barrier)
        }))
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        barrier.wait().await;

        for _ in 0..connections {
            tokio::net::TcpStream::connect(listener_addr).await.unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }

        barrier.wait().await;
    });

    let mut num_accepts_total = 0;

    for handle in handles {
        num_accepts_total += handle.join().unwrap();
    }

    num_accepts_total
}

fn count_accepts(std: std::net::TcpListener, flags: u32, barrier: Arc<Barrier>) -> u64 {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        std.set_nonblocking(true).unwrap();

        let listener = tokio::net::TcpListener::from_std_with_epoll_flags(std, flags).unwrap();

        barrier.wait().await;

        let mut barr_wait = std::pin::pin!(barrier.wait());

        loop {
            tokio::select! {
                _ = &mut barr_wait => {
                    return tokio::runtime::Handle::current().metrics().io_driver_ready_count();
                }
                a = listener.accept() => {
                    a.unwrap();
                }
            }
        }
    })
}
