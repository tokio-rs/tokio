#[cfg(feature = "incoming")]
use super::incoming::Incoming;
use super::TcpStream;
use mio;
use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::{self, SocketAddr};
use std::task::{Context, Poll};
use tokio_reactor::{Handle, PollEvented};

/// An I/O object representing a TCP socket listening for incoming connections.
///
/// This object can be converted into a stream of incoming connections for
/// various forms of processing.
///
/// # Examples
///
/// ```no_run
/// use futures::stream::Stream;
/// use std::net::SocketAddr;
/// use tokio::net::{TcpListener, TcpStream};
///
/// fn process_socket(socket: TcpStream) {
///     // ...
/// }
///
/// let addr = "127.0.0.1:8080".parse::<SocketAddr>()?;
/// let listener = TcpListener::bind(&addr)?;
///
/// // accept connections and process them
/// tokio::run(listener.incoming()
///     .map_err(|e| eprintln!("failed to accept socket; error = {:?}", e))
///     .for_each(|socket| {
///         process_socket(socket);
///         Ok(())
///     })
/// );
/// # Ok::<_, Box<dyn std::error::Error>>(())
/// ```
pub struct TcpListener {
    io: PollEvented<mio::net::TcpListener>,
}

impl TcpListener {
    /// Create a new TCP listener associated with this event loop.
    ///
    /// The TCP listener will bind to the provided `addr` address, if available.
    /// If the result is `Ok`, the socket has successfully bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::net::SocketAddr;
    /// use tokio::net::TcpListener;
    ///
    /// let addr = "127.0.0.1:0".parse::<SocketAddr>()?;
    /// let listener = TcpListener::bind(&addr)?;
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn bind(addr: &SocketAddr) -> io::Result<TcpListener> {
        let l = mio::net::TcpListener::bind(addr)?;
        Ok(TcpListener::new(l))
    }

    /// Attempt to accept a connection and create a new connected `TcpStream` if
    /// successful.
    ///
    /// Note that typically for simple usage it's easier to treat incoming
    /// connections as a `Stream` of `TcpStream`s with the `incoming` method
    /// below.
    ///
    /// # Return
    ///
    /// On success, returns `Ok(Async::Ready((socket, addr)))`.
    ///
    /// If the listener is not ready to accept, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the listener becomes ready to accept.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::SocketAddr;
    /// use tokio::net::TcpListener;
    /// use futures::Async;
    ///
    /// let addr = "127.0.0.1:0".parse::<SocketAddr>()?;
    /// let mut listener = TcpListener::bind(&addr)?;
    /// match listener.poll_accept() {
    ///     Ok(Async::Ready((_socket, addr))) => println!("listener ready to accept: {:?}", addr),
    ///     Ok(Async::NotReady) => println!("listener not ready to accept!"),
    ///     Err(e) => eprintln!("got an error: {}", e),
    /// }
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn poll_accept(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(TcpStream, SocketAddr)>> {
        let (io, addr) = ready!(self.poll_accept_std(cx))?;

        let io = mio::net::TcpStream::from_stream(io)?;
        let io = TcpStream::new(io);

        Poll::Ready(Ok((io, addr)))
    }

    /// Attempt to accept a connection and create a new connected `TcpStream` if
    /// successful.
    ///
    /// This function is the same as `accept` above except that it returns a
    /// `std::net::TcpStream` instead of a `tokio::net::TcpStream`. This in turn
    /// can then allow for the TCP stream to be associated with a different
    /// reactor than the one this `TcpListener` is associated with.
    ///
    /// # Return
    ///
    /// On success, returns `Ok(Async::Ready((socket, addr)))`.
    ///
    /// If the listener is not ready to accept, the method returns
    /// `Ok(Async::NotReady)` and arranges for the current task to receive a
    /// notification when the listener becomes ready to accept.
    ///
    /// # Panics
    ///
    /// This function will panic if called from outside of a task context.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use std::net::SocketAddr;
    /// use tokio::net::TcpListener;
    /// use futures::Async;
    ///
    /// let addr = "127.0.0.1:0".parse::<SocketAddr>()?;
    /// let mut listener = TcpListener::bind(&addr)?;
    /// match listener.poll_accept_std() {
    ///     Ok(Async::Ready((_socket, addr))) => println!("listener ready to accept: {:?}", addr),
    ///     Ok(Async::NotReady) => println!("listener not ready to accept!"),
    ///     Err(e) => eprintln!("got an error: {}", e),
    /// }
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn poll_accept_std(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<io::Result<(net::TcpStream, SocketAddr)>> {
        ready!(self.io.poll_read_ready(cx, mio::Ready::readable()))?;

        match self.io.get_ref().accept_std() {
            Ok(pair) => Poll::Ready(Ok(pair)),
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(cx, mio::Ready::readable())?;
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e)),
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
    /// Finally, the `handle` argument is the event loop that this listener will
    /// be bound to.
    /// Use [`Handle::default()`] to lazily bind to an event loop, just like `bind` does.
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
    /// [`Handle::default()`]: ../reactor/struct.Handle.html
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::TcpListener;
    /// use std::net::TcpListener as StdTcpListener;
    /// use tokio::reactor::Handle;
    ///
    /// let std_listener = StdTcpListener::bind("127.0.0.1:0")?;
    /// let listener = TcpListener::from_std(std_listener, &Handle::default())?;
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn from_std(listener: net::TcpListener, handle: &Handle) -> io::Result<TcpListener> {
        let io = mio::net::TcpListener::from_std(listener)?;
        let io = PollEvented::new_with_handle(io, handle)?;
        Ok(TcpListener { io })
    }

    fn new(listener: mio::net::TcpListener) -> TcpListener {
        let io = PollEvented::new(listener);
        TcpListener { io }
    }

    /// Returns the local address that this listener is bound to.
    ///
    /// This can be useful, for example, when binding to port 0 to figure out
    /// which port was actually bound.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::TcpListener;
    /// use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
    ///
    /// let addr = "127.0.0.1:8080".parse::<SocketAddr>()?;
    /// let listener = TcpListener::bind(&addr)?;
    /// assert_eq!(listener.local_addr()?,
    ///            SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080)));
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Consumes this listener, returning a stream of the sockets this listener
    /// accepts.
    ///
    /// This method returns an implementation of the `Stream` trait which
    /// resolves to the sockets the are accepted on this listener.
    ///
    /// # Errors
    ///
    /// Note that accepting a connection can lead to various errors and not all of them are
    /// necessarily fatal ‒ for example having too many open file descriptors or the other side
    /// closing the connection while it waits in an accept queue. These would terminate the stream
    /// if not handled in any way.
    ///
    /// If aiming for production, decision what to do about them must be made. The
    /// [`tk-listen`](https://crates.io/crates/tk-listen) crate might be of some help.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::TcpListener;
    /// use futures::stream::Stream;
    /// use std::net::SocketAddr;
    ///
    /// let addr = "127.0.0.1:0".parse::<SocketAddr>()?;
    /// let listener = TcpListener::bind(&addr)?;
    ///
    /// listener.incoming()
    ///     .map_err(|e| eprintln!("failed to accept stream; error = {:?}", e))
    ///     .for_each(|_socket| {
    ///         println!("new socket!");
    ///         Ok(())
    ///     });
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    #[cfg(feature = "incoming")]
    pub fn incoming(self) -> Incoming {
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
    /// ```
    /// use tokio::net::TcpListener;
    /// use std::net::SocketAddr;
    ///
    /// let addr = "127.0.0.1:0".parse::<SocketAddr>()?;
    /// let listener = TcpListener::bind(&addr)?;
    ///
    /// listener.set_ttl(100).expect("could not set TTL");
    ///
    /// assert_eq!(listener.ttl()?, 100);
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn ttl(&self) -> io::Result<u32> {
        self.io.get_ref().ttl()
    }

    /// Sets the value for the `IP_TTL` option on this socket.
    ///
    /// This value sets the time-to-live field that is used in every packet sent
    /// from this socket.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::TcpListener;
    /// use std::net::SocketAddr;
    ///
    /// let addr = "127.0.0.1:0".parse::<SocketAddr>()?;
    /// let listener = TcpListener::bind(&addr)?;
    ///
    /// listener.set_ttl(100).expect("could not set TTL");
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_ttl(ttl)
    }
}

impl TryFrom<TcpListener> for mio::net::TcpListener {
    type Error = io::Error;

    /// Consumes value, returning the mio I/O object.
    ///
    /// See [`tokio_reactor::PollEvented::into_inner`] for more details about
    /// resource deregistration that happens during the call.
    fn try_from(value: TcpListener) -> Result<Self, Self::Error> {
        value.io.into_inner()
    }
}

impl fmt::Debug for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

#[cfg(unix)]
mod sys {
    use super::TcpListener;
    use std::os::unix::prelude::*;

    impl AsRawFd for TcpListener {
        fn as_raw_fd(&self) -> RawFd {
            self.io.get_ref().as_raw_fd()
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
