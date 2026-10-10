#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use futures::task::{noop_waker_ref, ArcWake};
use futures::FutureExt;
use std::io::IoSlice;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Waker};
use tokio::io::{
    duplex, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf,
};
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok};

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

/// Returns a waker that calls `f` when woken, like an executor that polls the
/// woken task right away.
fn waker_fn(f: impl Fn() + Send + Sync + 'static) -> Waker {
    struct WakeFn<F>(F);

    impl<F: Fn() + Send + Sync + 'static> ArcWake for WakeFn<F> {
        fn wake_by_ref(arc_self: &Arc<Self>) {
            (arc_self.0)();
        }
    }

    futures::task::waker(Arc::new(WakeFn(f)))
}

#[test]
fn wake_reader_outside_lock() {
    let ops: [fn(DuplexStream); 4] = [
        |mut writer| assert_eq!(writer.write(b"x").now_or_never().unwrap().unwrap(), 1),
        |mut writer| {
            let bufs = [IoSlice::new(b"x")];
            let n = writer
                .write_vectored(&bufs)
                .now_or_never()
                .unwrap()
                .unwrap();
            assert_eq!(n, 1);
        },
        |mut writer| writer.shutdown().now_or_never().unwrap().unwrap(),
        drop,
    ];

    for op in ops {
        let (writer, reader) = duplex(1);
        let reader = Arc::new(Mutex::new(reader));
        let woken = Arc::new(AtomicBool::new(false));

        let waker = waker_fn({
            let reader = reader.clone();
            let woken = woken.clone();
            move || {
                let mut buf = [0; 1];
                let mut reader = reader.lock().unwrap();
                assert!(reader.read(&mut buf).now_or_never().is_some());
                woken.store(true, Ordering::SeqCst);
            }
        });

        let mut buf = [0; 1];
        let mut buf = ReadBuf::new(&mut buf);
        let mut cx = Context::from_waker(&waker);
        assert_pending!(Pin::new(&mut *reader.lock().unwrap()).poll_read(&mut cx, &mut buf));

        op(writer);
        assert!(woken.load(Ordering::SeqCst));
    }
}

#[test]
fn wake_writer_outside_lock() {
    let ops: [fn(DuplexStream); 2] = [
        |mut reader| assert_eq!(reader.read(&mut [0; 1]).now_or_never().unwrap().unwrap(), 1),
        drop,
    ];

    for op in ops {
        let (writer, reader) = duplex(1);
        let writer = Arc::new(Mutex::new(writer));
        let woken = Arc::new(AtomicBool::new(false));

        let waker = waker_fn({
            let writer = writer.clone();
            let woken = woken.clone();
            move || {
                let mut writer = writer.lock().unwrap();
                assert!(writer.write(b"y").now_or_never().is_some());
                woken.store(true, Ordering::SeqCst);
            }
        });

        let mut writer_guard = writer.lock().unwrap();
        assert_eq!(writer_guard.write(b"x").now_or_never().unwrap().unwrap(), 1);
        let mut cx = Context::from_waker(&waker);
        assert_pending!(Pin::new(&mut *writer_guard).poll_write(&mut cx, b"y"));
        drop(writer_guard);

        op(reader);
        assert!(woken.load(Ordering::SeqCst));
    }
}

#[test]
fn drop_replaced_waker_outside_lock() {
    // A waker that owns the other end of the pipe.
    struct Owner {
        _peer: DuplexStream,
    }

    impl ArcWake for Owner {
        fn wake_by_ref(_: &Arc<Self>) {}
    }

    let mut cx = Context::from_waker(noop_waker_ref());

    let (mut reader, peer) = duplex(1);
    let waker = futures::task::waker(Arc::new(Owner { _peer: peer }));
    let mut buf = [0; 1];
    let mut buf = ReadBuf::new(&mut buf);
    assert_pending!(Pin::new(&mut reader).poll_read(&mut Context::from_waker(&waker), &mut buf));
    drop(waker);
    // Replacing the waker drops the other end, which closes the pipe.
    assert_pending!(Pin::new(&mut reader).poll_read(&mut cx, &mut buf));
    assert_ready_ok!(Pin::new(&mut reader).poll_read(&mut cx, &mut buf));
    assert!(buf.filled().is_empty());

    let (mut writer, peer) = duplex(1);
    assert_ready_ok!(Pin::new(&mut writer).poll_write(&mut cx, b"x"));
    let waker = futures::task::waker(Arc::new(Owner { _peer: peer }));
    assert_pending!(Pin::new(&mut writer).poll_write(&mut Context::from_waker(&waker), b"y"));
    drop(waker);
    assert_pending!(Pin::new(&mut writer).poll_write(&mut cx, b"y"));
    assert_ready_err!(Pin::new(&mut writer).poll_write(&mut cx, b"y"));

    let (mut writer, peer) = duplex(1);
    assert_ready_ok!(Pin::new(&mut writer).poll_write(&mut cx, b"x"));
    let waker = futures::task::waker(Arc::new(Owner { _peer: peer }));
    let bufs = [IoSlice::new(b"y")];
    assert_pending!(
        Pin::new(&mut writer).poll_write_vectored(&mut Context::from_waker(&waker), &bufs)
    );
    drop(waker);
    assert_pending!(Pin::new(&mut writer).poll_write_vectored(&mut cx, &bufs));
    assert_ready_err!(Pin::new(&mut writer).poll_write_vectored(&mut cx, &bufs));
}
