#![warn(rust_2018_idioms)]
#![cfg(any(
    feature = "full",
    all(
        target_os = "emscripten",
        feature = "rt",
        feature = "macros",
        feature = "io-util"
    )
))]

use futures::task::noop_waker_ref;
use futures::FutureExt;
use std::future::Future;
use std::io::IoSlice;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Wake, Waker};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio_test::{assert_pending, assert_ready};

#[tokio::test]
async fn ping_pong() {
    let (mut a, mut b) = duplex(32);

    let mut buf = [0u8; 4];

    a.write_all(b"ping").await.unwrap();
    b.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"ping");

    b.write_all(b"pong").await.unwrap();
    a.read_exact(&mut buf).await.unwrap();
    assert_eq!(&buf, b"pong");
}

#[tokio::test]
async fn across_tasks() {
    let (mut a, mut b) = duplex(32);

    let t1 = tokio::spawn(async move {
        a.write_all(b"ping").await.unwrap();
        let mut buf = [0u8; 4];
        a.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"pong");
    });

    let t2 = tokio::spawn(async move {
        let mut buf = [0u8; 4];
        b.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"ping");
        b.write_all(b"pong").await.unwrap();
    });

    t1.await.unwrap();
    t2.await.unwrap();
}

#[tokio::test]
async fn disconnect() {
    let (mut a, mut b) = duplex(32);

    let t1 = tokio::spawn(async move {
        a.write_all(b"ping").await.unwrap();
        // and dropped
    });

    let t2 = tokio::spawn(async move {
        let mut buf = [0u8; 32];
        let n = b.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping");

        let n = b.read(&mut buf).await.unwrap();
        assert_eq!(n, 0);
    });

    t1.await.unwrap();
    t2.await.unwrap();
}

#[tokio::test]
async fn disconnect_reader() {
    let (a, mut b) = duplex(2);

    let t1 = tokio::spawn(async move {
        // this will block, as not all data fits into duplex
        b.write_all(b"ping").await.unwrap_err();
    });

    let t2 = tokio::spawn(async move {
        // here we drop the reader side, and we expect the writer in the other
        // task to exit with an error
        drop(a);
    });

    t2.await.unwrap();
    t1.await.unwrap();
}

#[tokio::test]
async fn max_write_size() {
    let (mut a, mut b) = duplex(32);

    let t1 = tokio::spawn(async move {
        let n = a.write(&[0u8; 64]).await.unwrap();
        assert_eq!(n, 32);
        let n = a.write(&[0u8; 64]).await.unwrap();
        assert_eq!(n, 4);
    });

    let mut buf = [0u8; 4];
    b.read_exact(&mut buf).await.unwrap();

    t1.await.unwrap();

    // drop b only after task t1 finishes writing
    drop(b);
}

#[tokio::test]
async fn zero_length_operations() {
    let (mut reader, _peer) = duplex(1);
    assert!(matches!(reader.read(&mut []).now_or_never(), Some(Ok(0))));

    let (mut writer, _peer) = duplex(1);
    writer.write_all(b"x").await.unwrap();
    assert!(matches!(writer.write(&[]).now_or_never(), Some(Ok(0))));

    let (mut writer, _peer) = duplex(1);
    writer.write_all(b"x").await.unwrap();
    let bufs = [IoSlice::new(&[]), IoSlice::new(&[])];
    assert!(matches!(
        writer.write_vectored(&bufs).now_or_never(),
        Some(Ok(0))
    ));
}

#[tokio::test]
async fn zero_length_writes_to_closed_stream() {
    let (mut writer, peer) = duplex(1);
    drop(peer);
    let err = writer.write(&[]).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);

    let (mut writer, peer) = duplex(1);
    drop(peer);
    let bufs = [IoSlice::new(&[]), IoSlice::new(&[])];
    let err = writer.write_vectored(&bufs).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::BrokenPipe);
}

#[tokio::test]
async fn duplex_is_cooperative() {
    let (mut tx, mut rx) = tokio::io::duplex(1024 * 8);

    tokio::select! {
        biased;

        _ = async {
            loop {
                let buf = [3u8; 4096];
                tx.write_all(&buf).await.unwrap();
                let mut buf = [0u8; 4096];
                let _ = rx.read(&mut buf).await.unwrap();
            }
        } => {},
        _ = tokio::task::yield_now() => {}
    }
}

/// Polls `fut` once (expecting `Pending`) with a waker that polls `fut` to
/// completion when woken.
fn poll_on_wake(fut: impl Future<Output = ()> + Send + 'static) -> Arc<AtomicBool> {
    struct PollOnWake<F> {
        fut: Mutex<Pin<Box<F>>>,
        woken: Arc<AtomicBool>,
    }

    impl<F: Future<Output = ()> + Send> Wake for PollOnWake<F> {
        fn wake(self: Arc<Self>) {
            let mut cx = Context::from_waker(noop_waker_ref());
            assert_ready!(self.fut.lock().unwrap().as_mut().poll(&mut cx));
            self.woken.store(true, Ordering::SeqCst);
        }
    }

    let woken = Arc::new(AtomicBool::new(false));
    let task = Arc::new(PollOnWake {
        fut: Mutex::new(Box::pin(fut)),
        woken: woken.clone(),
    });
    let waker = Waker::from(task.clone());
    let mut cx = Context::from_waker(&waker);
    assert_pending!(task.fut.lock().unwrap().as_mut().poll(&mut cx));
    woken
}

#[tokio::test]
async fn wake_reader_on_write_outside_lock() {
    let (mut writer, mut reader) = duplex(1);
    let woken = poll_on_wake(async move {
        assert_eq!(reader.read(&mut [0; 1]).await.unwrap(), 1);
    });

    assert_eq!(writer.write(b"x").await.unwrap(), 1);
    assert!(woken.load(Ordering::SeqCst));
}

#[tokio::test]
async fn wake_reader_on_write_vectored_outside_lock() {
    let (mut writer, mut reader) = duplex(1);
    let woken = poll_on_wake(async move {
        assert_eq!(reader.read(&mut [0; 1]).await.unwrap(), 1);
    });

    assert_eq!(
        writer.write_vectored(&[IoSlice::new(b"x")]).await.unwrap(),
        1
    );
    assert!(woken.load(Ordering::SeqCst));
}

#[tokio::test]
async fn wake_reader_on_shutdown_outside_lock() {
    let (mut writer, mut reader) = duplex(1);
    let woken = poll_on_wake(async move {
        assert_eq!(reader.read(&mut [0; 1]).await.unwrap(), 0);
    });

    writer.shutdown().await.unwrap();
    assert!(woken.load(Ordering::SeqCst));
}

#[test]
fn wake_reader_on_drop_outside_lock() {
    let (writer, mut reader) = duplex(1);
    let woken = poll_on_wake(async move {
        assert_eq!(reader.read(&mut [0; 1]).await.unwrap(), 0);
    });

    drop(writer);
    assert!(woken.load(Ordering::SeqCst));
}

#[tokio::test]
async fn wake_writer_on_read_outside_lock() {
    let (mut writer, mut reader) = duplex(1);
    let woken = poll_on_wake(async move {
        writer.write_all(b"xy").await.unwrap();
    });

    assert_eq!(reader.read(&mut [0; 1]).await.unwrap(), 1);
    assert!(woken.load(Ordering::SeqCst));
}

#[test]
fn wake_writer_on_drop_outside_lock() {
    let (mut writer, reader) = duplex(1);
    let woken = poll_on_wake(async move {
        writer.write_all(b"xy").await.unwrap_err();
    });

    drop(reader);
    assert!(woken.load(Ordering::SeqCst));
}

fn check_drop_replaced_waker(peer: DuplexStream, fut: impl Future<Output = ()>) {
    struct Owner {
        _peer: DuplexStream,
    }

    impl Wake for Owner {
        fn wake(self: Arc<Self>) {}
    }

    let mut fut = pin!(fut);
    let waker = Waker::from(Arc::new(Owner { _peer: peer }));
    assert_pending!(fut.as_mut().poll(&mut Context::from_waker(&waker)));
    drop(waker);

    let mut cx = Context::from_waker(noop_waker_ref());
    // Replacing the waker drops `peer`, which closes the pipe.
    assert_pending!(fut.as_mut().poll(&mut cx));
    assert_ready!(fut.as_mut().poll(&mut cx));
}

#[test]
fn drop_replaced_read_waker_outside_lock() {
    let (mut reader, peer) = duplex(1);
    check_drop_replaced_waker(peer, async {
        assert_eq!(reader.read(&mut [0; 1]).await.unwrap(), 0);
    });
}

#[test]
fn drop_replaced_write_waker_outside_lock() {
    let (mut writer, peer) = duplex(1);
    check_drop_replaced_waker(peer, async {
        writer.write_all(b"xy").await.unwrap_err();
    });
}

#[test]
fn drop_replaced_write_vectored_waker_outside_lock() {
    let (mut writer, peer) = duplex(1);
    check_drop_replaced_waker(peer, async {
        assert_eq!(writer.write(b"x").await.unwrap(), 1);
        writer
            .write_vectored(&[IoSlice::new(b"y")])
            .await
            .unwrap_err();
    });
}
