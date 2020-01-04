use crate::future::poll_fn;
use crate::io::{AsyncRead, AsyncWrite};
use crate::simulation::tcp::Link;
use std::{future::Future, io, net, pin::Pin, sync, task::Context, task::Poll, time};

/// Inner shared state for two halves of a TcpStream.
#[derive(Debug)]
struct Shared {
    client_addr: net::SocketAddr,
    server_addr: net::SocketAddr,
    client_shutdown: Option<net::Shutdown>,
    server_shutdown: Option<net::Shutdown>,
    client_read_latency: Option<time::Duration>,
    client_write_latency: Option<time::Duration>,
    server_read_latency: Option<time::Duration>,
    server_write_latency: Option<time::Duration>,
}

#[derive(Debug)]
pub enum TcpStreamHandleError {
    StreamDropped,
}

#[derive(Debug)]
pub struct SimTcpStreamHandle {
    shared: sync::Weak<sync::Mutex<Shared>>,
}

impl SimTcpStreamHandle {
    fn new(shared: sync::Weak<sync::Mutex<Shared>>) -> Self {
        Self { shared }
    }
    fn shared(&self) -> Result<sync::Arc<sync::Mutex<Shared>>, TcpStreamHandleError> {
        self.shared
            .upgrade()
            .ok_or(TcpStreamHandleError::StreamDropped)
    }

    pub(crate) fn dropped(&self) -> bool {
        self.shared.upgrade().is_none()
    }

    pub fn set_client_read_latency(
        &self,
        latency: Option<time::Duration>,
    ) -> Result<(), TcpStreamHandleError> {
        let shared = self.shared()?;
        let mut lock = shared.lock().unwrap();
        lock.client_read_latency = latency;
        Ok(())
    }

    pub fn set_client_write_latency(
        &self,
        latency: Option<time::Duration>,
    ) -> Result<(), TcpStreamHandleError> {
        let shared = self.shared()?;
        let mut lock = shared.lock().unwrap();
        lock.client_write_latency = latency;
        Ok(())
    }

    pub fn set_server_read_latency(
        &self,
        latency: Option<time::Duration>,
    ) -> Result<(), TcpStreamHandleError> {
        let shared = self.shared()?;
        let mut lock = shared.lock().unwrap();
        lock.server_read_latency = latency;
        Ok(())
    }

    pub fn set_server_write_latency(
        &self,
        latency: Option<time::Duration>,
    ) -> Result<(), TcpStreamHandleError> {
        let shared = self.shared()?;
        let mut lock = shared.lock().unwrap();
        lock.server_write_latency = latency;
        Ok(())
    }
}

#[derive(Debug)]
pub struct SimTcpStream {
    local_addr: net::SocketAddr,
    peer_addr: net::SocketAddr,
    shared: sync::Arc<sync::Mutex<Shared>>,
    link: Link,
    read_delay: Option<crate::time::Delay>,
    write_delay: Option<crate::time::Delay>,
}

impl SimTcpStream {
    fn new(
        local_addr: net::SocketAddr,
        peer_addr: net::SocketAddr,
        shared: sync::Arc<sync::Mutex<Shared>>,
        link: Link,
    ) -> Self {
        Self {
            local_addr,
            peer_addr,
            shared,
            link,
            read_delay: None,
            write_delay: None,
        }
    }

    pub(crate) fn new_pair(
        client_addr: net::SocketAddr,
        server_addr: net::SocketAddr,
    ) -> (Self, Self, SimTcpStreamHandle) {
        let (client_link, server_link) = Link::new_pair();
        let shared = Shared {
            client_addr,
            server_addr,
            client_shutdown: None,
            server_shutdown: None,
            client_read_latency: None,
            client_write_latency: None,
            server_read_latency: None,
            server_write_latency: None,
        };
        let shared = sync::Arc::new(sync::Mutex::new(shared));
        let client = SimTcpStream::new(
            client_addr,
            server_addr,
            sync::Arc::clone(&shared),
            client_link,
        );
        let server = SimTcpStream::new(
            server_addr,
            client_addr,
            sync::Arc::clone(&shared),
            server_link,
        );
        let weak = sync::Arc::downgrade(&shared);
        let handle = SimTcpStreamHandle::new(weak);
        (client, server, handle)
    }

    pub fn local_addr(&self) -> io::Result<net::SocketAddr> {
        Ok(self.local_addr)
    }

    pub fn peer_addr(&self) -> io::Result<net::SocketAddr> {
        Ok(self.peer_addr)
    }

    pub fn poll_peek(&mut self, cx: &mut Context<'_>, buf: &mut [u8]) -> Poll<io::Result<usize>> {
        self.link.poll_peek(cx, buf)
    }

    pub async fn peek(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        poll_fn(|cx| self.poll_peek(cx, buf)).await
    }

    pub fn shutdown(&self, how: net::Shutdown) -> io::Result<()> {
        if self.is_client() {
            self.shared.lock().unwrap().client_shutdown.replace(how);
        } else {
            self.shared.lock().unwrap().server_shutdown.replace(how);
        }
        Ok(())
    }

    pub(crate) fn nodelay(&self) -> io::Result<bool> {
        todo!()
    }

    pub(crate) fn set_nodelay(&self, nodelay: bool) -> io::Result<()> {
        todo!()
    }

    pub(crate) fn recv_buffer_size(&self) -> io::Result<usize> {
        todo!()
    }

    pub(crate) fn set_recv_buffer_size(&self, size: usize) -> io::Result<()> {
        todo!()
    }

    pub(crate) fn send_buffer_size(&self) -> io::Result<usize> {
        todo!()
    }

    pub(crate) fn set_send_buffer_size(&self, size: usize) -> io::Result<()> {
        todo!()
    }

    pub(crate) fn keepalive(&self) -> io::Result<Option<time::Duration>> {
        todo!()
    }

    pub(crate) fn set_keepalive(&self, keepalive: Option<time::Duration>) -> io::Result<()> {
        todo!()
    }

    pub(crate) fn ttl(&self) -> io::Result<u32> {
        todo!()
    }

    pub(crate) fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        todo!()
    }

    pub(crate) fn linger(&self) -> io::Result<Option<time::Duration>> {
        todo!()
    }

    pub(crate) fn set_linger(&self, dur: Option<time::Duration>) -> io::Result<()> {
        todo!()
    }
}

impl SimTcpStream {
    fn is_client(&self) -> bool {
        self.shared.lock().unwrap().client_addr == self.local_addr
    }

    fn shutdown_status(&self) -> Option<net::Shutdown> {
        if self.is_client() {
            self.shared.lock().unwrap().client_shutdown
        } else {
            self.shared.lock().unwrap().server_shutdown
        }
    }

    fn poll_read_delay(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(mut delay) = self.read_delay.take() {
            match Pin::new(&mut delay).poll(cx) {
                Poll::Pending => {
                    self.read_delay.replace(delay.into());
                    return Poll::Pending;
                }
                Poll::Ready(_) => return Poll::Ready(Ok(())),
            }
        } else {
            let is_client = self.as_ref().is_client();
            let lock = self.shared.lock().unwrap();
            let read_latency = if is_client {
                lock.client_read_latency
            } else {
                lock.server_read_latency
            };

            drop(lock);
            read_latency.map(|delay| self.read_delay = Some(crate::time::delay_for(delay)));
            Poll::Ready(Ok(()))
        }
    }

    fn poll_write_delay(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        if let Some(mut delay) = self.write_delay.take() {
            match Pin::new(&mut delay).poll(cx) {
                Poll::Pending => {
                    self.write_delay.replace(delay.into());
                    return Poll::Pending;
                }
                Poll::Ready(_) => return Poll::Ready(Ok(())),
            }
        } else {
            let is_client = self.as_ref().is_client();
            let lock = self.shared.lock().unwrap();
            let write_latency = if is_client {
                lock.client_write_latency
            } else {
                lock.server_write_latency
            };
            drop(lock);
            write_latency.map(|delay| self.write_delay = Some(crate::time::delay_for(delay)));
            Poll::Ready(Ok(()))
        }
    }
}

impl AsyncRead for SimTcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        if let Some(net::Shutdown::Read) = self.shutdown_status() {
            return Poll::Ready(Ok(0));
        }
        ready!(self.as_mut().poll_read_delay(cx))?;
        Pin::new(&mut self.link).poll_read(cx, buf)
    }
}

impl AsyncWrite for SimTcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        if let Some(net::Shutdown::Write) = self.shutdown_status() {
            return Poll::Ready(Err(io::ErrorKind::ConnectionAborted.into()));
        }
        ready!(self.as_mut().poll_write_delay(cx))?;
        Pin::new(&mut self.link).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.link).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<(), io::Error>> {
        Pin::new(&mut self.link).poll_shutdown(cx)
    }
}
