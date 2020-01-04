use crate::future::poll_fn;
use crate::simulation::tcp::SimTcpStream;
use futures_core::Stream;
use std::{
    io, net,
    pin::Pin,
    sync,
    task::{Context, Poll},
};

use crate::sync::mpsc;
/// Fault injection state for the TcpListener
#[derive(Debug)]
struct Shared {}
#[derive(Debug)]
pub struct SimTcpListener {
    local_addr: net::SocketAddr,
    incoming: mpsc::Receiver<SimTcpStream>,
    shared: sync::Arc<sync::Mutex<Shared>>,
}

impl Incoming<'_> {
    pub(crate) fn new(listener: &mut SimTcpListener) -> Incoming<'_> {
        Incoming { inner: listener }
    }

    #[doc(hidden)] // TODO: dox
    pub fn poll_accept(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<SimTcpStream>> {
        let (socket, _) = ready!(self.inner.poll_accept(cx))?;
        Poll::Ready(Ok(socket))
    }
}

impl Stream for Incoming<'_> {
    type Item = io::Result<SimTcpStream>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(self.inner.poll_accept(cx))?;
        Poll::Ready(Some(Ok(socket)))
    }
}

/// Stream returned by the `TcpListener::incoming` function representing the
/// stream of sockets received from a listener.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Incoming<'a> {
    inner: &'a mut SimTcpListener,
}

impl SimTcpListener {
    pub(crate) fn new(local_addr: net::SocketAddr) -> (Self, SimTclListenerHandle) {
        let (tx, rx) = mpsc::channel(1);
        let shared = Shared {};
        let shared = sync::Arc::new(sync::Mutex::new(shared));
        let weak = sync::Arc::downgrade(&shared);
        (
            Self {
                local_addr,
                shared,
                incoming: rx,
            },
            SimTclListenerHandle {
                shared: weak,
                sender: tx,
            },
        )
    }

    pub fn local_addr(&self) -> io::Result<net::SocketAddr> {
        Ok(self.local_addr)
    }

    pub async fn accept(&mut self) -> io::Result<(SimTcpStream, net::SocketAddr)> {
        poll_fn(|cx| self.poll_accept(cx)).await
    }

    pub fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(SimTcpStream, net::SocketAddr), io::Error>> {
        let stream =
            ready!(Pin::new(&mut self.incoming).poll_next(cx)).ok_or(io::ErrorKind::NotFound)?;
        let addr = stream.peer_addr()?;
        Poll::Ready(Ok((stream, addr)))
    }

    pub fn incoming(&mut self) -> Incoming<'_> {
        Incoming::new(self)
    }

    pub fn ttl(&self) -> io::Result<u32> {
        Ok(0)
    }

    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct SimTclListenerHandle {
    shared: sync::Weak<sync::Mutex<Shared>>,
    sender: mpsc::Sender<SimTcpStream>,
}

impl SimTclListenerHandle {
    pub fn dropped(&self) -> bool {
        self.shared.upgrade().is_none()
    }

    pub async fn enqueue_incoming(&mut self, stream: SimTcpStream) -> Result<(), io::Error> {
        self.sender
            .send(stream)
            .await
            .map_err(|_| io::ErrorKind::ConnectionRefused)?;
        Ok(())
    }
}
