//! `LocalEventLoop` driven by the Emscripten host loop itself. `main` spawns
//! tasks on two hosted event loops and returns; from there the tasks
//! progress only through the drives the loops schedule on the host: for
//! timer deadlines, for host-context wakes (including a cross-loop oneshot),
//! between the batches of a self-yielding task that must not starve either,
//! and, with `net`, for epoll readiness in a TCP round trip between a server
//! on one loop and a client on the other, then a server whose client is a
//! Node socket connecting after every tokio timer has fired: for that
//! stretch only the event loop's own hold on the runtime keeps the process
//! alive.
//!
//! `harness = false`, so `main` can return into the host loop. Runs both with
//! and without `-sJSPI`; without it any stack suspension traps, so completion
//! proves the drives never suspend. Each event loop holds the Emscripten
//! runtime alive while it has tasks; once the last task completes the runtime
//! exits. If the tasks stall instead, the loop drains without a runtime exit
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
    use std::future::Future;
    use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
    use std::time::{Duration, Instant};

    use tokio::runtime::{Builder, LocalEventLoop};

    static COMPLETED: AtomicU32 = AtomicU32::new(0);
    static EXPECTED: AtomicU32 = AtomicU32::new(0);

    extern "C" {
        fn emscripten_run_script(script: *const std::ffi::c_char);
    }

    fn run_js(script: &str) {
        let script = std::ffi::CString::new(script).unwrap();
        // SAFETY: a NUL-terminated script evaluated on the host.
        unsafe { emscripten_run_script(script.as_ptr()) }
    }

    fn event_loop() -> &'static LocalEventLoop {
        // The event loops outlive `main`: the drives they schedule on the
        // host are what run the tasks.
        Box::leak(Box::new(
            Builder::new_current_thread()
                .enable_all()
                .event_interval(4)
                .build_hosted_local_event_loop(Default::default())
                .unwrap(),
        ))
    }

    fn root(el: &LocalEventLoop, fut: impl Future<Output = ()> + 'static) {
        EXPECTED.fetch_add(1, SeqCst);
        el.spawn_local(async move {
            fut.await;
            let done = COMPLETED.fetch_add(1, SeqCst) + 1;
            if done == EXPECTED.load(SeqCst) {
                println!("ok: {done} roots completed via host drives");
                run_js("Module.tokioDone = true");
            }
        });
    }

    pub(super) fn main() {
        run_js("Module.tokioExpectDone = true");
        let el_a = event_loop();
        let el_b = event_loop();
        let (tx, rx) = tokio::sync::oneshot::channel::<u32>();

        // A `block_on` that would wait drops its future, and with it the
        // timer it registered: nothing may stay armed for it, or the process
        // would live on to that deadline (`rt_emscripten_pre.js` bounds the
        // run).
        assert!(el_a
            .block_on(async { tokio::time::sleep(Duration::from_secs(30)).await })
            .is_err());
        run_js("Module.tokioDeadline = Date.now() + 10_000");

        // More spawns than one batch runs: the drive the hosted loop
        // schedules for the leftovers must run them, or the loop would hold
        // the runtime alive with nothing armed.
        for _ in 0..6 {
            root(el_b, async {});
        }

        root(el_a, async move {
            // A real deadline: only the host timer can resume this.
            let start = Instant::now();
            tokio::time::sleep(Duration::from_millis(20)).await;
            assert!(start.elapsed() >= Duration::from_millis(15));

            // A greedy sibling: each batch returns to the host, so the
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

            // Wakes the task parked on the other loop: sent from a drive of
            // this one, picked up by the drive b schedules.
            tx.send(7).unwrap();
        });

        root(el_b, async move {
            tokio::time::sleep(Duration::from_millis(5)).await;
            assert_eq!(rx.await.unwrap(), 7);
        });

        #[cfg(feature = "net")]
        {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            use tokio::net::{TcpListener, TcpStream};

            let (addr_tx, addr_rx) = tokio::sync::oneshot::channel();

            root(el_a, async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                addr_tx.send(listener.local_addr().unwrap()).unwrap();
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 5];
                sock.read_exact(&mut buf).await.unwrap();
                assert_eq!(&buf, b"hello");
                sock.write_all(b"world").await.unwrap();
            });

            root(el_b, async move {
                let addr = addr_rx.await.unwrap();
                let mut sock = TcpStream::connect(addr).await.unwrap();
                sock.write_all(b"hello").await.unwrap();
                let mut buf = [0u8; 5];
                sock.read_exact(&mut buf).await.unwrap();
                assert_eq!(&buf, b"world");
            });

            // A client outside tokio, connecting long after the last tokio
            // timer: until it does, only the event loop's hold on the
            // runtime (it has a live task) keeps the process alive.
            root(el_a, async move {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let port = listener.local_addr().unwrap().port();
                run_js(&format!(
                    "setTimeout(() => {{ const s = require('net').connect({port}, '127.0.0.1'); \
                     s.on('error', () => {{}}); s.on('connect', () => s.end('late')); }}, 300)"
                ));
                let (mut sock, _) = listener.accept().await.unwrap();
                let mut buf = [0u8; 4];
                sock.read_exact(&mut buf).await.unwrap();
                assert_eq!(&buf, b"late");
            });
        }

        // Return into the host loop: from here each loop advances only on
        // its own host callbacks.
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
