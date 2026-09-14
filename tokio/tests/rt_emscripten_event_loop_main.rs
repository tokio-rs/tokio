//! `EventLoopRuntime` driven by the host loop itself. `main` schedules roots
//! on two runtimes and returns; from there the roots progress only through
//! host callbacks: timer deadlines, immediate drives for host-context wakes
//! (including a cross-runtime oneshot), a self-yielding task that must not
//! starve either, and, with `net`, a TCP round trip between a server root on
//! one runtime and a client on the other, re-driven by epoll readiness.
//!
//! `harness = false`, so `main` can return into the host loop. Runs both with
//! and without `-sJSPI`; without it any stack suspension traps, so completion
//! proves the drives never suspend. Once the roots complete nothing holds the
//! host loop (no timer armed, no socket open) and the Emscripten runtime
//! exits; if the roots stall instead, the loop drains without a runtime exit
//! and `rt_emscripten_pre.js` fails the run.

#[cfg(all(
    target_os = "emscripten",
    not(target_feature = "atomics"),
    tokio_unstable,
    feature = "rt",
    feature = "time",
    feature = "sync"
))]
mod emscripten {
    use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
    use std::time::{Duration, Instant};

    use tokio::runtime::{Builder, EventLoopRuntime};

    static COMPLETED: AtomicU32 = AtomicU32::new(0);
    static EXPECTED: AtomicU32 = AtomicU32::new(0);

    fn event_loop_rt() -> &'static EventLoopRuntime {
        // The runtimes outlive `main`: their armed host callbacks are what
        // drive the roots.
        Box::leak(Box::new(
            Builder::new_current_thread()
                .enable_all()
                .build_event_loop_runtime()
                .unwrap(),
        ))
    }

    fn complete<T>(out: Result<T, tokio::task::JoinError>) {
        out.unwrap();
        let done = COMPLETED.fetch_add(1, SeqCst) + 1;
        if done == EXPECTED.load(SeqCst) {
            println!("ok: {done} roots completed via host drives");
        }
    }

    pub(super) fn main() {
        let rt_a = event_loop_rt();
        let rt_b = event_loop_rt();
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

        EXPECTED.fetch_add(1, SeqCst);
        rt_a.schedule(
            async move {
                // A real deadline: only the armed host timer can resume this.
                let start = Instant::now();
                tokio::time::sleep(Duration::from_millis(20)).await;
                assert!(start.elapsed() >= Duration::from_millis(15));

                // A greedy sibling: each batch yields a host turn, so the
                // timer below still fires.
                let greedy = tokio::spawn(async {
                    loop {
                        tokio::task::yield_now().await;
                    }
                });
                let start = Instant::now();
                tokio::time::sleep(Duration::from_millis(10)).await;
                assert!(start.elapsed() >= Duration::from_millis(5));
                greedy.abort();

                // Wakes the root parked on the other runtime: sent from a
                // drive of this one, picked up by an immediate drive of b.
                tx.send(7).unwrap();
            },
            complete,
        );

        EXPECTED.fetch_add(1, SeqCst);
        rt_b.schedule(
            async move {
                tokio::time::sleep(Duration::from_millis(5)).await;
                assert_eq!(rx.await.unwrap(), 7);
            },
            complete,
        );

        #[cfg(feature = "net")]
        {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::{TcpListener, TcpStream};

            let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

            EXPECTED.fetch_add(1, SeqCst);
            rt_a.schedule(
                async move {
                    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                    addr_tx.send(listener.local_addr().unwrap()).unwrap();
                    let (mut sock, _) = listener.accept().await.unwrap();
                    let mut buf = [0u8; 5];
                    sock.read_exact(&mut buf).await.unwrap();
                    assert_eq!(&buf, b"hello");
                    sock.write_all(b"world").await.unwrap();
                },
                complete,
            );

            EXPECTED.fetch_add(1, SeqCst);
            rt_b.schedule(
                async move {
                    let addr = addr_rx.await.unwrap();
                    let mut sock = TcpStream::connect(addr).await.unwrap();
                    sock.write_all(b"hello").await.unwrap();
                    let mut buf = [0u8; 5];
                    sock.read_exact(&mut buf).await.unwrap();
                    assert_eq!(&buf, b"world");
                },
                complete,
            );
        }

        // Return into the host loop: from here each runtime advances only
        // on its own host callbacks.
    }
}

fn main() {
    #[cfg(all(
        target_os = "emscripten",
        not(target_feature = "atomics"),
        tokio_unstable,
        feature = "rt",
        feature = "time",
        feature = "sync"
    ))]
    emscripten::main();
}
