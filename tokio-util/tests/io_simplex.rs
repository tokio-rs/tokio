use futures::pin_mut;
use futures_test::task::noop_context;
use std::{future::Future, task::Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf};
use tokio_test::{assert_pending, assert_ready_err};
use tokio_util::io::simplex;

/// Sanity check for single-threaded operation.
#[tokio::test]
async fn single_thread() {
    const N: usize = 64;
    const MSG: &[u8] = b"Hello, world!";

    let (mut tx, mut rx) = simplex::new(32);

    for _ in 0..N {
        tx.write_all(MSG).await.unwrap();
        let mut buf = vec![0; MSG.len()];
        rx.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf[..], MSG);
    }
}

/// Sanity check for multi-threaded operation.
#[test]
#[cfg(not(target_os = "wasi"))] // No thread on wasi.
fn multi_thread() {
    use futures::executor::block_on;
    use std::thread;

    const N: usize = 64;
    const MSG: &[u8] = b"Hello, world!";

    let (mut tx, mut rx) = simplex::new(32);

    let jh0 = thread::spawn(move || {
        block_on(async {
            let mut buf = vec![0; MSG.len()];
            for _ in 0..N {
                rx.read_exact(&mut buf).await.unwrap();
                assert_eq!(&buf[..], MSG);
                buf.clear();
                buf.resize(MSG.len(), 0);
            }
        });
    });

    let jh1 = thread::spawn(move || {
        block_on(async {
            for _ in 0..N {
                tx.write_all(MSG).await.unwrap();
            }
        });
    });

    jh0.join().unwrap();
    jh1.join().unwrap();
}

/// The `Sender` should returns error if the read half has been dropped.
#[tokio::test]
async fn drop_receiver_0() {
    const MSG: &[u8] = b"Hello, world!";

    let (mut tx, rx) = simplex::new(32);
    drop(rx);

    tx.write_all(MSG).await.unwrap_err();
}

/// The `Sender` should be woken up if the read half is dropped.
#[tokio::test]
async fn drop_receiver_1() {
    const MSG: &[u8] = b"Hello, world!";

    // only set `1` capacity to make sure the write half will be blocked
    // by the read half
    let (mut tx, rx) = simplex::new(1);
    let fut = tx.write_all(MSG);
    pin_mut!(fut);
    assert_pending!(fut.as_mut().poll(&mut noop_context()));
    drop(rx);
    assert_ready_err!(fut.poll(&mut noop_context()));
}

/// The `Receiver` should returns error if:
///
/// - The `Sender` has been dropped.
/// - AND there is no remaining data in the buffer.
#[tokio::test]
async fn drop_sender_0() {
    const MSG: &[u8] = b"Hello, world!";

    let (tx, mut rx) = simplex::new(32);
    drop(tx);

    let mut buf = vec![0; MSG.len()];
    rx.read_exact(&mut buf).await.unwrap_err();
}

/// The `Receiver` should be woken up if:
///
/// - The `Sender` has been dropped.
/// - AND there is no sufficient data to read.
#[tokio::test]
async fn drop_sender_1() {
    const MSG: &[u8] = b"Hello, world!";

    // only set `1` capacity to make sure the write half will be blocked
    // by the read half
    let (tx, mut rx) = simplex::new(1);
    let mut buf = [0u8; MSG.len()];
    let fut = rx.read_exact(&mut buf);
    pin_mut!(fut);
    assert_pending!(fut.as_mut().poll(&mut noop_context()));
    drop(tx);
    assert_ready_err!(fut.poll(&mut noop_context()));
}

/// Both `Sender` and `Receiver` should yield periodically
/// in a tight-loop.
#[tokio::test]
async fn cooperative_scheduling() {
    // this magic number is copied from
    // https://github.com/tokio-rs/tokio/blob/925c614c89d0a26777a334612e2ed6ad0e7935c3/tokio/src/task/coop/mod.rs#L116
    const INITIAL_BUDGET: usize = 128;

    let (tx, _rx) = simplex::new(INITIAL_BUDGET * 2);
    pin_mut!(tx);
    let mut is_pending = false;
    for _ in 0..INITIAL_BUDGET + 1 {
        match tx.as_mut().poll_write(&mut noop_context(), &[0u8; 1]) {
            Poll::Pending => {
                is_pending = true;
                break;
            }
            Poll::Ready(Ok(1)) => {}
            Poll::Ready(Ok(n)) => panic!("wrote too many bytes: {n}"),
            Poll::Ready(Err(e)) => panic!("{e}"),
        }
    }
    assert!(is_pending);

    let (mut tx, rx) = simplex::new(INITIAL_BUDGET * 2);
    tx.write_all(&[0u8; INITIAL_BUDGET + 2]).await.unwrap();
    pin_mut!(rx);
    let mut is_pending = false;
    for _ in 0..INITIAL_BUDGET + 1 {
        let mut buf = [0u8; 1];
        let mut buf = ReadBuf::new(&mut buf);
        match rx.as_mut().poll_read(&mut noop_context(), &mut buf) {
            Poll::Pending => {
                is_pending = true;
                break;
            }
            Poll::Ready(Ok(())) => assert_eq!(buf.filled().len(), 1),
            Poll::Ready(Err(e)) => panic!("{e}"),
        }
    }
    assert!(is_pending);
}
