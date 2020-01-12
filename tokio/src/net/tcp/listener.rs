use crate::future::poll_fn;
use crate::io::PollEvented;
use crate::net::tcp::{Incoming, TcpStream};
use crate::net::ToSocketAddrs;
use crate::runtime::context;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::{self, SocketAddr};
use std::task::{Context, Poll};

cfg_tcp! {

    pub(crate) enum ListenerInner {
        Mio(PollEvented<mio::net::TcpListener>),
        Sim(crate::simulation::tcp::SimTcpListener),
    }
    /// A TCP socket server, listening for connections.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    ///
    /// use std::io;
    ///
    /// async fn process_socket<T>(socket: T) {
    ///     # drop(socket);
    ///     // do work with socket here
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         let (socket, _) = listener.accept().await?;
    ///         process_socket(socket).await;
    ///     }
    /// }
    /// ```
    #[derive(Debug)]
    pub struct TcpListener {
        io: ListenerInner,
    }
}

impl TcpListener {
    /// Creates a new TcpListener which will be bound to the specified address.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of 0 will request that the OS assigns a port
    /// to this listener. The port allocated can be queried via the `local_addr`
    /// method.
    ///
    /// The address type can be any implementor of `ToSocketAddrs` trait.
    ///
    /// If `addr` yields multiple addresses, bind will be attempted with each of
    /// the addresses until one succeeds and returns the listener. If none of
    /// the addresses succeed in creating a listener, the error returned from
    /// the last attempt (the last address) is returned.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let listener = TcpListener::bind("127.0.0.1:0").await?;
    ///
    ///     // use the listener
    ///
    ///     # let _ = listener;
    ///     Ok(())
    /// }
    /// ```
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        let addrs = addr.to_socket_addrs().await?;

        let mut last_err = None;

        for addr in addrs {
            match context::tcp_listener_bind_addr(addr).expect("no reactor") {
                Ok(listener) => return Ok(TcpListener { io: listener }),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not resolve to any addresses",
            )
        }))
    }

    /// Accept a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`TcpStream`] and the remote peer's
    /// address will be returned.
    ///
    /// [`TcpStream`]: ../struct.TcpStream.html
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///     match listener.accept().await {
    ///         Ok((_socket, addr)) => println!("new client: {:?}", addr),
    ///         Err(e) => println!("couldn't get client: {:?}", e),
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept(&mut self) -> io::Result<(TcpStream, SocketAddr)> {
        poll_fn(|cx| self.poll_accept(cx)).await
    }

    #[doc(hidden)] // TODO: document
    pub fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(TcpStream, SocketAddr)>> {
        match &mut self.io {
            ListenerInner::Mio(m) => {
                // this used to be poll_accept_std
                ready!(m.poll_read_ready(cx, mio::Ready::readable()))?;
                let (io, addr) = match m.get_ref().accept_std() {
                    Ok(pair) => pair,
                    Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                        m.clear_read_ready(cx, mio::Ready::readable())?;
                        return Poll::Pending;
                    }
                    Err(e) => return Poll::Ready(Err(e)),
                };

                let io = mio::net::TcpStream::from_stream(io)?;
                let io = TcpStream::from_mio(io)?;

                Poll::Ready(Ok((io, addr)))
            }
            ListenerInner::Sim(s) => match s.poll_accept(cx) {
                Poll::Ready(Ok((stream, addr))) => {
                    Poll::Ready(Ok((TcpStream::new(stream.into()), addr)))
                }
                Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
                Poll::Pending => Poll::Pending,
            },
        }
    }

    /// Create a new TCP listener from the standard library's TCP listener.
    ///
    /// This method can be used when the `Handle::tcp_listen` method isn't
    /// sufficient because perhaps some more configuration is needed in terms of
    /// before the calls to `bind` and `listen`.
    ///
    /// This API is typically paired with the `net2` crate and the `TcpBuilder`
    /// type to build up and customize a listener before it's shipped off to the
    /// backing event loop. This allows configuration of options like
    /// `SO_REUSEPORT`, binding to multiple addresses, etc.
    ///
    /// The `addr` argument here is one of the addresses that `listener` is
    /// bound to and the listener will only be guaranteed to accept connections
    /// of the same address type currently.
    ///
    /// The platform specific behavior of this function looks like:
    ///
    /// * On Unix, the socket is placed into nonblocking mode and connections
    ///   can be accepted as normal
    ///
    /// * On Windows, the address is stored internally and all future accepts
    ///   will only be for the same IP version as `addr` specified. That is, if
    ///   `addr` is an IPv4 address then all sockets accepted will be IPv4 as
    ///   well (same for IPv6).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use std::error::Error;
    /// use tokio::net::TcpListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn Error>> {
    ///     let std_listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    ///     let listener = TcpListener::from_std(std_listener)?;
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Handle::enter`](crate::runtime::Handle::enter) function.
    pub fn from_std(listener: net::TcpListener) -> io::Result<TcpListener> {
        let io = mio::net::TcpListener::from_std(listener)?;
        let registration = context::register_io(&io).expect("no reactor")?;
        let io = PollEvented::new(io, registration)?;
        Ok(TcpListener { io: io.into() })
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use tokio::net::TcpListener;
    ///
    /// use std::io;
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///     assert_eq!(listener.local_addr()?,
    ///                SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        match &self.io {
            ListenerInner::Mio(m) => m.get_ref().local_addr(),
            ListenerInner::Sim(s) => s.local_addr(),
        }
    }

    /// Returns a stream over the connections being received on this listener.
    ///
    /// The returned stream will never return `None` and will also not yield the
    /// peer's `SocketAddr` structure. Iterating over it is equivalent to
    /// calling accept in a loop.
    ///
    /// # Errors
    ///
    /// Note that accepting a connection can lead to various errors and not all
    /// of them are necessarily fatal ‒ for example having too many open file
    /// descriptors or the other side closing the connection while it waits in
    /// an accept queue. These would terminate the stream if not handled in any
    /// way.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::{net::TcpListener, stream::StreamExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    ///     let mut incoming = listener.incoming();
    ///
    ///     while let Some(stream) = incoming.next().await {
    ///         match stream {
    ///             Ok(stream) => {
    ///                 println!("new client!");
    ///             }
    ///             Err(e) => { /* connection failed */ }
    ///         }
    ///     }
    /// }
    /// ```
    pub fn incoming(&mut self) -> Incoming<'_> {
        Incoming::new(self)
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: #method.set_ttl
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///    let listener = TcpListener::bind("127.0.0.1:0").await?;
    ///
    ///    listener.set_ttl(100).expect("could not set TTL");
    ///    assert_eq!(listener.ttl()?, 100);
    ///
    ///    Ok(())
    /// }
    /// ```
    pub fn ttl(&self) -> io::Result<u32> {
        match &self.io {
            ListenerInner::Mio(m) => m.get_ref().ttl(),
            ListenerInner::Sim(s) => s.ttl(),
        }
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    ///
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let listener = TcpListener::bind("127.0.0.1:0").await?;
    ///
    ///     listener.set_ttl(100).expect("could not set TTL");
    ///
    ///     Ok(())
    /// }
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        match &self.io {
            ListenerInner::Mio(m) => m.get_ref().set_ttl(ttl),
            ListenerInner::Sim(s) => s.set_ttl(ttl),
        }
    }
}

impl TryFrom<TcpListener> for mio::net::TcpListener {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    ///
    /// [`PollEvented::into_inner`]: crate::io::PollEvented::into_inner
    fn try_from(value: TcpListener) -> Result<Self, Self::Error> {
        if let ListenerInner::Mio(m) = value.io {
            Ok(m.into_inner()?)
        } else {
            Err(io::Error::new(
                io::ErrorKind::Other,
                "cannot get mio listener from simulation type",
            ))
        }
    }
}

impl TryFrom<net::TcpListener> for TcpListener {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`TcpListener::from_std(stream)`](TcpListener::from_std).
    fn try_from(stream: net::TcpListener) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

impl fmt::Debug for ListenerInner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ListenerInner::Mio(m) => m.get_ref().fmt(f),
            ListenerInner::Sim(s) => s.fmt(f),
        }
    }
}

#[cfg(unix)]
mod sys {
    use super::{ListenerInner, TcpListener};
    use std::os::unix::prelude::*;

    impl AsRawFd for TcpListener {
        fn as_raw_fd(&self) -> RawFd {
            if let ListenerInner::Mio(m) = &self.io {
                m.get_ref().as_raw_fd()
            } else {
                panic!("cannot get RawFd for simulation types")
            }
        }
    }
}

#[cfg(windows)]
mod sys {
    // TODO: let's land these upstream with mio and then we can add them here.
    //
    // use std::os::windows::prelude::*;
    // use super::{TcpListener;
    //
    // impl AsRawHandle for TcpListener {
    //     fn as_raw_handle(&self) -> RawHandle {
    //         self.listener.io().as_raw_handle()
    //     }
    // }
}

// ===== impl StreamInner =====

impl From<PollEvented<mio::net::TcpListener>> for ListenerInner {
    fn from(item: PollEvented<mio::net::TcpListener>) -> Self {
        ListenerInner::Mio(item)
    }
}

impl From<crate::simulation::tcp::SimTcpListener> for ListenerInner {
    fn from(item: crate::simulation::tcp::SimTcpListener) -> Self {
        ListenerInner::Sim(item)
    }
}
