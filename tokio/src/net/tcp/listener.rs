use crate::net::tcp::TcpStream;
use crate::net::ToSocketAddrs;

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::{self, SocketAddr};
use std::task::{Context, Poll};

cfg_net! {
    /// A TCP socket server, listening for connections.
    ///
    /// You can accept a new connection by using the [`accept`](`TcpListener::accept`) method. Alternatively `TcpListener`
    /// implements the [`Stream`](`crate::stream::Stream`) trait, which allows you to use the listener in places that want a
    /// stream. The stream will never return `None` and will also not yield the peer's `SocketAddr` structure.  Iterating over
    /// it is equivalent to calling accept in a loop.
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
    /// Using `accept`:
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
    ///     let listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         let (socket, _) = listener.accept().await?;
    ///         process_socket(socket).await;
    ///     }
    /// }
    /// ```
    ///
    /// Using `impl Stream`:
    /// ```no_run
    /// use tokio::{net::TcpListener, stream::StreamExt};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let mut listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();
    ///     while let Some(stream) = listener.next().await {
    ///         match stream {
    ///             Ok(stream) => {
    ///                 println!("new client!");
    ///             }
    ///             Err(e) => { /* connection failed */ }
    ///         }
    ///     }
    /// }
    /// ```
    pub struct TcpListener(pub(crate) t10::net::TcpListener);
}

impl TcpListener {
    /// Creates a new TcpListener, which will be bound to the specified address.
    ///
    /// The returned listener is ready for accepting connections.
    ///
    /// Binding with a port number of 0 will request that the OS assigns a port
    /// to this listener. The port allocated can be queried via the `local_addr`
    /// method.
    ///
    /// The address type can be any implementor of the [`ToSocketAddrs`] trait.
    /// Note that strings only implement this trait when the **`net`** feature
    /// is enabled, as strings may contain domain names that need to be resolved.
    ///
    /// If `addr` yields multiple addresses, bind will be attempted with each of
    /// the addresses until one succeeds and returns the listener. If none of
    /// the addresses succeed in creating a listener, the error returned from
    /// the last attempt (the last address) is returned.
    ///
    /// This function sets the `SO_REUSEADDR` option on the socket.
    ///
    /// [`ToSocketAddrs`]: trait@crate::net::ToSocketAddrs
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
    ///     let listener = TcpListener::bind("127.0.0.1:2345").await?;
    ///
    ///     // use the listener
    ///
    ///     # let _ = listener;
    ///     Ok(())
    /// }
    /// ```
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<TcpListener> {
        t10::net::TcpListener::bind(addr).await.map(Self)
    }

    /// Accepts a new incoming connection from this listener.
    ///
    /// This function will yield once a new TCP connection is established. When
    /// established, the corresponding [`TcpStream`] and the remote peer's
    /// address will be returned.
    ///
    /// [`TcpStream`]: struct@crate::net::TcpStream
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
    ///     let listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///     match listener.accept().await {
    ///         Ok((_socket, addr)) => println!("new client: {:?}", addr),
    ///         Err(e) => println!("couldn't get client: {:?}", e),
    ///     }
    ///
    ///     Ok(())
    /// }
    /// ```
    pub async fn accept(&self) -> io::Result<(TcpStream, SocketAddr)> {
        let (t10_stream, addr) = self.0.accept().await?;

        Ok((TcpStream(t10_stream), addr))
    }

    /// Polls to accept a new incoming connection to this listener.
    ///
    /// If there is no connection to accept, `Poll::Pending` is returned and the
    /// current task will be notified by a waker.
    ///
    /// When ready, the most recent task that called `poll_accept` is notified.
    /// The caller is responsible to ensure that `poll_accept` is called from a
    /// single task. Failing to do this could result in tasks hanging.
    pub fn poll_accept(&self, cx: &mut Context<'_>) -> Poll<io::Result<(TcpStream, SocketAddr)>> {
        self.0
            .poll_accept(cx)
            .map(|res| res.map(|(s, addr)| (TcpStream(s), addr)))
    }

    /// Creates new `TcpListener` from a `std::net::TcpListener`.
    ///
    /// This function is intended to be used to wrap a TCP listener from the
    /// standard library in the Tokio equivalent. The conversion assumes nothing
    /// about the underlying listener; it is left up to the user to set it in
    /// non-blocking mode.
    ///
    /// This API is typically paired with the `socket2` crate and the `Socket`
    /// type to build up and customize a listener before it's shipped off to the
    /// backing event loop. This allows configuration of options like
    /// `SO_REUSEPORT`, binding to multiple addresses, etc.
    ///
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
    ///     std_listener.set_nonblocking(true)?;
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
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    pub fn from_std(listener: net::TcpListener) -> io::Result<TcpListener> {
        t10::net::TcpListener::from_std(listener).map(Self)
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
    #[inline]
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.0.local_addr()
    }

    /// Gets the value of the `IP_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_ttl`].
    ///
    /// [`set_ttl`]: method@Self::set_ttl
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
    #[inline]
    pub fn ttl(&self) -> io::Result<u32> {
        self.0.ttl()
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
    #[inline]
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.0.set_ttl(ttl)
    }
}

#[cfg(feature = "stream")]
impl crate::stream::Stream for TcpListener {
    type Item = io::Result<TcpStream>;

    fn poll_next(self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(self.poll_accept(cx))?;
        Poll::Ready(Some(Ok(socket)))
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

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(unix)]
mod sys {
    use super::TcpListener;
    use std::os::unix::prelude::*;

    impl AsRawFd for TcpListener {
        fn as_raw_fd(&self) -> RawFd {
            self.0.as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    use super::TcpListener;
    use std::os::windows::prelude::*;

    impl AsRawSocket for TcpListener {
        fn as_raw_socket(&self) -> RawSocket {
            self.0.as_raw_socket()
        }
    }
}
