#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, oneshot};
use tokio_test::assert_ok;

use std::net::{IpAddr, SocketAddr};

macro_rules! test_accept {
    ($(($ident:ident, $target:expr),)*) => {
        $(
            #[tokio::test]
            async fn $ident() {
                let mut listener = assert_ok!(TcpListener::bind($target).await);
                let addr = listener.local_addr().unwrap();

                let (tx, rx) = oneshot::channel();

                tokio::spawn(async move {
                    let (socket, _) = assert_ok!(listener.accept().await);
                    assert_ok!(tx.send(socket));
                });

                let cli = assert_ok!(TcpStream::connect(&addr).await);
                let srv = assert_ok!(rx.await);

                assert_eq!(cli.local_addr().unwrap(), srv.peer_addr().unwrap());
            }
        )*
    }
}

test_accept! {
    (ip_str, "127.0.0.1:0"),
    (host_str, "localhost:0"),
    (socket_addr, "127.0.0.1:0".parse::<SocketAddr>().unwrap()),
    (str_port_tuple, ("127.0.0.1", 0)),
    (ip_port_tuple, ("127.0.0.1".parse::<IpAddr>().unwrap(), 0)),
}

use std::pin::Pin;
use std::sync::{
    atomic::{AtomicUsize, Ordering::SeqCst},
    Arc,
};
use std::task::{Context, Poll};
use tokio::stream::{Stream, StreamExt};

struct TrackPolls<S> {
    npolls: Arc<AtomicUsize>,
    s: S,
}

impl<S> Stream for TrackPolls<S>
where
    S: Stream,
{
    type Item = S::Item;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // safety: we do not move s
        let this = unsafe { self.get_unchecked_mut() };
        this.npolls.fetch_add(1, SeqCst);
        // safety: we are pinned, and so is s
        unsafe { Pin::new_unchecked(&mut this.s) }.poll_next(cx)
    }
}

#[tokio::test]
async fn no_extra_poll() {
    let mut listener = assert_ok!(TcpListener::bind("127.0.0.1:0").await);
    let addr = listener.local_addr().unwrap();

    let (tx, rx) = oneshot::channel();
    let (accepted_tx, mut accepted_rx) = mpsc::unbounded_channel();

    tokio::spawn(async move {
        let mut incoming = TrackPolls {
            npolls: Arc::new(AtomicUsize::new(0)),
            s: listener.incoming(),
        };
        assert_ok!(tx.send(Arc::clone(&incoming.npolls)));
        while let Some(_) = incoming.next().await {
            accepted_tx.send(()).unwrap();
        }
    });

    let npolls = assert_ok!(rx.await);
    tokio::task::yield_now().await;

    // should have been polled exactly once: the initial poll
    assert_eq!(npolls.load(SeqCst), 1);

    let _ = assert_ok!(TcpStream::connect(&addr).await);
    accepted_rx.next().await.unwrap();

    // should have been polled twice more: once to yield Some(), then once to yield Pending
    assert_eq!(npolls.load(SeqCst), 1 + 2);
}
