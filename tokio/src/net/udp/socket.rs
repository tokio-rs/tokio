use crate::io::{PollEvented, ReadBuf};
use crate::net::{to_socket_addrs, ToSocketAddrs};

use std::convert::TryFrom;
use std::fmt;
use std::io;
use std::net::{self, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::task::{Context, Poll};

cfg_net! {
    /// A UDP socket
    ///
    /// UDP is "connectionless", unlike TCP. Meaning, regardless of what address you've bound to, a `UdpSocket`
    /// is free to communicate with many different remotes. In tokio there are basically two main ways to use `UdpSocket`:
    ///
    /// * one to many: [`bind`](`UdpSocket::bind`) and use [`send_to`](`UdpSocket::send_to`)
    ///   and [`recv_from`](`UdpSocket::recv_from`) to communicate with many different addresses
    /// * one to one: [`connect`](`UdpSocket::connect`) and associate with a single address, using [`send`](`UdpSocket::send`)
    ///   and [`recv`](`UdpSocket::recv`) to communicate only with that remote address
    ///
    /// `UdpSocket` can also be used concurrently to `send_to` and `recv_from` in different tasks,
    /// all that's required is that you `Arc<UdpSocket>` and clone a reference for each task.
    ///
    /// # Streams
    ///
    /// If you need to listen over UDP and produce a [`Stream`](`crate::stream::Stream`), you can look
    /// at [`UdpFramed`].
    ///
    /// [`UdpFramed`]: https://docs.rs/tokio-util/latest/tokio_util/udp/struct.UdpFramed.html
    ///
    /// # Example: one to many (bind)
    ///
    /// Using `bind` we can create a simple echo server that sends and recv's with many different clients:
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let sock = UdpSocket::bind("0.0.0.0:8080").await?;
    ///     let mut buf = [0; 1024];
    ///     loop {
    ///         let (len, addr) = sock.recv_from(&mut buf).await?;
    ///         println!("{:?} bytes received from {:?}", len, addr);
    ///
    ///         let len = sock.send_to(&buf[..len], addr).await?;
    ///         println!("{:?} bytes sent", len);
    ///     }
    /// }
    /// ```
    ///
    /// # Example: one to one (connect)
    ///
    /// Or using `connect` we can echo with a single remote address using `send` and `recv`:
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let sock = UdpSocket::bind("0.0.0.0:8080").await?;
    ///
    ///     let remote_addr = "127.0.0.1:59611";
    ///     sock.connect(remote_addr).await?;
    ///     let mut buf = [0; 1024];
    ///     loop {
    ///         let len = sock.recv(&mut buf).await?;
    ///         println!("{:?} bytes received from {:?}", len, remote_addr);
    ///
    ///         let len = sock.send(&buf[..len]).await?;
    ///         println!("{:?} bytes sent", len);
    ///     }
    /// }
    /// ```
    ///
    /// # Example: Sending/Receiving concurrently
    ///
    /// Because `send_to` and `recv_from` take `&self`. It's perfectly alright to `Arc<UdpSocket>`
    /// and share the references to multiple tasks, in order to send/receive concurrently. Here is
    /// a similar "echo" example but that supports concurrent sending/receiving:
    ///
    /// ```no_run
    /// use tokio::{net::UdpSocket, sync::mpsc};
    /// use std::{io, net::SocketAddr, sync::Arc};
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let sock = UdpSocket::bind("0.0.0.0:8080".parse::<SocketAddr>().unwrap()).await?;
    ///     let r = Arc::new(sock);
    ///     let s = r.clone();
    ///     let (tx, mut rx) = mpsc::channel::<(Vec<u8>, SocketAddr)>(1_000);
    ///
    ///     tokio::spawn(async move {
    ///         while let Some((bytes, addr)) = rx.recv().await {
    ///             let len = s.send_to(&bytes, &addr).await.unwrap();
    ///             println!("{:?} bytes sent", len);
    ///         }
    ///     });
    ///
    ///     let mut buf = [0; 1024];
    ///     loop {
    ///         let (len, addr) = r.recv_from(&mut buf).await?;
    ///         println!("{:?} bytes received from {:?}", len, addr);
    ///         tx.send((buf[..len].to_vec(), addr)).await.unwrap();
    ///     }
    /// }
    /// ```
    ///
    pub struct UdpSocket {
        io: PollEvented<mio::net::UdpSocket>,
    }
}

impl UdpSocket {
    /// This function will create a new UDP socket and attempt to bind it to
    /// the `addr` provided.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// use std::io;
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let sock = UdpSocket::bind("0.0.0.0:8080").await?;
    ///     // use `sock`
    /// #   let _ = sock;
    ///     Ok(())
    /// }
    /// ```
    pub async fn bind<A: ToSocketAddrs>(addr: A) -> io::Result<UdpSocket> {
        let addrs = to_socket_addrs(addr).await?;
        let mut last_err = None;

        for addr in addrs {
            match UdpSocket::bind_addr(addr) {
                Ok(socket) => return Ok(socket),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not resolve to any address",
            )
        }))
    }

    fn bind_addr(addr: SocketAddr) -> io::Result<UdpSocket> {
        let sys = mio::net::UdpSocket::bind(addr)?;
        UdpSocket::new(sys)
    }

    fn new(socket: mio::net::UdpSocket) -> io::Result<UdpSocket> {
        let io = PollEvented::new(socket)?;
        Ok(UdpSocket { io })
    }

    /// Creates new `UdpSocket` from a previously bound `std::net::UdpSocket`.
    ///
    /// This function is intended to be used to wrap a UDP socket from the
    /// standard library in the Tokio equivalent. The conversion assumes nothing
    /// about the underlying socket; it is left up to the user to set it in
    /// non-blocking mode.
    ///
    /// This can be used in conjunction with socket2's `Socket` interface to
    /// configure a socket before it's handed off, such as setting options like
    /// `reuse_address` or binding to multiple addresses.
    ///
    /// # Panics
    ///
    /// This function panics if thread-local runtime is not set.
    ///
    /// The runtime is usually set implicitly when this function is called
    /// from a future driven by a tokio runtime, otherwise runtime can be set
    /// explicitly with [`Runtime::enter`](crate::runtime::Runtime::enter) function.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let addr = "0.0.0.0:8080".parse::<SocketAddr>().unwrap();
    /// let std_sock = std::net::UdpSocket::bind(addr)?;
    /// let sock = UdpSocket::from_std(std_sock)?;
    /// // use `sock`
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_std(socket: net::UdpSocket) -> io::Result<UdpSocket> {
        let io = mio::net::UdpSocket::from_std(socket);
        UdpSocket::new(io)
    }

    /// Returns the local address that this socket is bound to.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let addr = "0.0.0.0:8080".parse::<SocketAddr>().unwrap();
    /// let sock = UdpSocket::bind(addr).await?;
    /// // the address the socket is bound to
    /// let local_addr = sock.local_addr()?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Connects the UDP socket setting the default destination for send() and
    /// limiting packets that are read via recv from the address specified in
    /// `addr`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let sock = UdpSocket::bind("0.0.0.0:8080".parse::<SocketAddr>().unwrap()).await?;
    ///
    /// let remote_addr = "127.0.0.1:59600".parse::<SocketAddr>().unwrap();
    /// sock.connect(remote_addr).await?;
    /// let mut buf = [0u8; 32];
    /// // recv from remote_addr
    /// let len = sock.recv(&mut buf).await?;
    /// // send to remote_addr
    /// let _len = sock.send(&buf[..len]).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn connect<A: ToSocketAddrs>(&self, addr: A) -> io::Result<()> {
        let addrs = to_socket_addrs(addr).await?;
        let mut last_err = None;

        for addr in addrs {
            match self.io.get_ref().connect(addr) {
                Ok(_) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "could not resolve to any address",
            )
        }))
    }

    /// Returns a future that sends data on the socket to the remote address to which it is connected.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will resolve to an error if the socket is not connected.
    ///
    /// [`connect`]: method@Self::connect
    pub async fn send(&self, buf: &[u8]) -> io::Result<usize> {
        self.io
            .async_io(mio::Interest::WRITABLE, |sock| sock.send(buf))
            .await
    }

    /// Attempts to send data on the socket to the remote address to which it was previously
    /// `connect`ed.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will resolve to an error if the socket is not connected.
    ///
    /// Note that on multiple calls to a `poll_*` method in the send direction, only the
    /// `Waker` from the `Context` passed to the most recent call will be scheduled to
    /// receive a wakeup.
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the socket is not available to write
    /// * `Poll::Ready(Ok(n))` `n` is the number of bytes sent
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    ///
    /// [`connect`]: method@Self::connect
    pub fn poll_send(&self, cx: &mut Context<'_>, buf: &[u8]) -> Poll<io::Result<usize>> {
        loop {
            let ev = ready!(self.io.poll_write_ready(cx))?;

            match self.io.get_ref().send(buf) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.clear_readiness(ev);
                }
                x => return Poll::Ready(x),
            }
        }
    }

    /// Try to send data on the socket to the remote address to which it is
    /// connected.
    ///
    /// # Returns
    ///
    /// If successfull, the number of bytes sent is returned. Users
    /// should ensure that when the remote cannot receive, the
    /// [`ErrorKind::WouldBlock`] is properly handled.
    ///
    /// [`ErrorKind::WouldBlock`]: std::io::ErrorKind::WouldBlock
    pub fn try_send(&self, buf: &[u8]) -> io::Result<usize> {
        self.io.get_ref().send(buf)
    }

    /// Returns a future that receives a single datagram message on the socket from
    /// the remote address to which it is connected. On success, the future will resolve
    /// to the number of bytes read.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size to
    /// hold the message bytes. If a message is too long to fit in the supplied buffer,
    /// excess bytes may be discarded.
    ///
    /// The [`connect`] method will connect this socket to a remote address. The future
    /// will fail if the socket is not connected.
    ///
    /// [`connect`]: method@Self::connect
    pub async fn recv(&self, buf: &mut [u8]) -> io::Result<usize> {
        self.io
            .async_io(mio::Interest::READABLE, |sock| sock.recv(buf))
            .await
    }

    /// Attempts to receive a single datagram message on the socket from the remote
    /// address to which it is `connect`ed.
    ///
    /// The [`connect`] method will connect this socket to a remote address. This method
    /// resolves to an error if the socket is not connected.
    ///
    /// Note that on multiple calls to a `poll_*` method in the recv direction, only the
    /// `Waker` from the `Context` passed to the most recent call will be scheduled to
    /// receive a wakeup.
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the socket is not ready to read
    /// * `Poll::Ready(Ok(()))` reads data `ReadBuf` if the socket is ready
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    ///
    /// [`connect`]: method@Self::connect
    pub fn poll_recv(&self, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>> {
        loop {
            let ev = ready!(self.io.poll_read_ready(cx))?;

            // Safety: will not read the maybe uinitialized bytes.
            let b = unsafe {
                &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8])
            };
            match self.io.get_ref().recv(b) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.clear_readiness(ev);
                }
                Err(e) => return Poll::Ready(Err(e)),
                Ok(n) => {
                    // Safety: We trust `recv` to have filled up `n` bytes
                    // in the buffer.
                    unsafe {
                        buf.assume_init(n);
                    }
                    buf.advance(n);
                    return Poll::Ready(Ok(()));
                }
            }
        }
    }

    /// Returns a future that sends data on the socket to the given address.
    /// On success, the future will resolve to the number of bytes written.
    ///
    /// The future will resolve to an error if the IP version of the socket does
    /// not match that of `target`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let sock = UdpSocket::bind("0.0.0.0:8080".parse::<SocketAddr>().unwrap()).await?;
    /// let buf = b"hello world";
    /// let remote_addr = "127.0.0.1:58000".parse::<SocketAddr>().unwrap();
    /// let _len = sock.send_to(&buf[..], remote_addr).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn send_to<A: ToSocketAddrs>(&self, buf: &[u8], target: A) -> io::Result<usize> {
        let mut addrs = to_socket_addrs(target).await?;

        match addrs.next() {
            Some(target) => self.send_to_addr(buf, target).await,
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "no addresses to send data to",
            )),
        }
    }

    /// Attempts to send data on the socket to a given address.
    ///
    /// Note that on multiple calls to a `poll_*` method in the send direction, only the
    /// `Waker` from the `Context` passed to the most recent call will be scheduled to
    /// receive a wakeup.
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the socket is not ready to write
    /// * `Poll::Ready(Ok(n))` `n` is the number of bytes sent.
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    pub fn poll_send_to(
        &self,
        cx: &mut Context<'_>,
        buf: &[u8],
        target: &SocketAddr,
    ) -> Poll<io::Result<usize>> {
        loop {
            let ev = ready!(self.io.poll_write_ready(cx))?;

            match self.io.get_ref().send_to(buf, *target) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.clear_readiness(ev);
                }
                x => return Poll::Ready(x),
            }
        }
    }

    /// Try to send data on the socket to the given address, but if the send is blocked
    /// this will return right away.
    ///
    /// # Returns
    ///
    /// If successfull, returns the number of bytes sent
    ///
    /// Users should ensure that when the remote cannot receive, the
    /// [`ErrorKind::WouldBlock`] is properly handled. An error can also occur
    /// if the IP version of the socket does not match that of `target`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let sock = UdpSocket::bind("0.0.0.0:8080".parse::<SocketAddr>().unwrap()).await?;
    /// let buf = b"hello world";
    /// let remote_addr = "127.0.0.1:58000".parse::<SocketAddr>().unwrap();
    /// let _len = sock.try_send_to(&buf[..], remote_addr)?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// [`ErrorKind::WouldBlock`]: std::io::ErrorKind::WouldBlock
    pub fn try_send_to(&self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        self.io.get_ref().send_to(buf, target)
    }

    async fn send_to_addr(&self, buf: &[u8], target: SocketAddr) -> io::Result<usize> {
        self.io
            .async_io(mio::Interest::WRITABLE, |sock| sock.send_to(buf, target))
            .await
    }

    /// Returns a future that receives a single datagram on the socket. On success,
    /// the future resolves to the number of bytes read and the origin.
    ///
    /// The function must be called with valid byte array `buf` of sufficient size
    /// to hold the message bytes. If a message is too long to fit in the supplied
    /// buffer, excess bytes may be discarded.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let sock = UdpSocket::bind("0.0.0.0:8080".parse::<SocketAddr>().unwrap()).await?;
    /// let mut buf = [0u8; 32];
    /// let (len, addr) = sock.recv_from(&mut buf).await?;
    /// println!("received {:?} bytes from {:?}", len, addr);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn recv_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.io
            .async_io(mio::Interest::READABLE, |sock| sock.recv_from(buf))
            .await
    }

    /// Attempts to receive a single datagram on the socket.
    ///
    /// Note that on multiple calls to a `poll_*` method in the recv direction, only the
    /// `Waker` from the `Context` passed to the most recent call will be scheduled to
    /// receive a wakeup.
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the socket is not ready to read
    /// * `Poll::Ready(Ok(addr))` reads data from `addr` into `ReadBuf` if the socket is ready
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    pub fn poll_recv_from(
        &self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<SocketAddr>> {
        loop {
            let ev = ready!(self.io.poll_read_ready(cx))?;

            // Safety: will not read the maybe uinitialized bytes.
            let b = unsafe {
                &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8])
            };
            match self.io.get_ref().recv_from(b) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.clear_readiness(ev);
                }
                Err(e) => return Poll::Ready(Err(e)),
                Ok((n, addr)) => {
                    // Safety: We trust `recv` to have filled up `n` bytes
                    // in the buffer.
                    unsafe {
                        buf.assume_init(n);
                    }
                    buf.advance(n);
                    return Poll::Ready(Ok(addr));
                }
            }
        }
    }

    /// Receives data from the socket, without removing it from the input queue.
    /// On success, returns the number of bytes read and the address from whence
    /// the data came.
    ///
    /// # Notes
    ///
    /// On Windows, if the data is larger than the buffer specified, the buffer
    /// is filled with the first part of the data, and peek_from returns the error
    /// WSAEMSGSIZE(10040). The excess data is lost.
    /// Make sure to always use a sufficiently large buffer to hold the
    /// maximum UDP packet size, which can be up to 65536 bytes in size.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::{io, net::SocketAddr};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> io::Result<()> {
    /// let sock = UdpSocket::bind("0.0.0.0:8080".parse::<SocketAddr>().unwrap()).await?;
    /// let mut buf = [0u8; 32];
    /// let (len, addr) = sock.peek_from(&mut buf).await?;
    /// println!("peeked {:?} bytes from {:?}", len, addr);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn peek_from(&self, buf: &mut [u8]) -> io::Result<(usize, SocketAddr)> {
        self.io
            .async_io(mio::Interest::READABLE, |sock| sock.peek_from(buf))
            .await
    }

    /// Receives data from the socket, without removing it from the input queue.
    /// On success, returns the number of bytes read.
    ///
    /// # Notes
    ///
    /// Note that on multiple calls to a `poll_*` method in the recv direction, only the
    /// `Waker` from the `Context` passed to the most recent call will be scheduled to
    /// receive a wakeup
    ///
    /// On Windows, if the data is larger than the buffer specified, the buffer
    /// is filled with the first part of the data, and peek returns the error
    /// WSAEMSGSIZE(10040). The excess data is lost.
    /// Make sure to always use a sufficiently large buffer to hold the
    /// maximum UDP packet size, which can be up to 65536 bytes in size.
    ///
    /// # Return value
    ///
    /// The function returns:
    ///
    /// * `Poll::Pending` if the socket is not ready to read
    /// * `Poll::Ready(Ok(addr))` reads data from `addr` into `ReadBuf` if the socket is ready
    /// * `Poll::Ready(Err(e))` if an error is encountered.
    ///
    /// # Errors
    ///
    /// This function may encounter any standard I/O error except `WouldBlock`.
    pub fn poll_peek_from(
        &self,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<SocketAddr>> {
        loop {
            let ev = ready!(self.io.poll_read_ready(cx))?;

            // Safety: will not read the maybe uinitialized bytes.
            let b = unsafe {
                &mut *(buf.unfilled_mut() as *mut [std::mem::MaybeUninit<u8>] as *mut [u8])
            };
            match self.io.get_ref().peek_from(b) {
                Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.clear_readiness(ev);
                }
                Err(e) => return Poll::Ready(Err(e)),
                Ok((n, addr)) => {
                    // Safety: We trust `recv` to have filled up `n` bytes
                    // in the buffer.
                    unsafe {
                        buf.assume_init(n);
                    }
                    buf.advance(n);
                    return Poll::Ready(Ok(addr));
                }
            }
        }
    }

    /// Gets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// For more information about this option, see [`set_broadcast`].
    ///
    /// [`set_broadcast`]: method@Self::set_broadcast
    pub fn broadcast(&self) -> io::Result<bool> {
        self.io.get_ref().broadcast()
    }

    /// Sets the value of the `SO_BROADCAST` option for this socket.
    ///
    /// When enabled, this socket is allowed to send packets to a broadcast
    /// address.
    pub fn set_broadcast(&self, on: bool) -> io::Result<()> {
        self.io.get_ref().set_broadcast(on)
    }

    /// Gets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see [`set_multicast_loop_v4`].
    ///
    /// [`set_multicast_loop_v4`]: method@Self::set_multicast_loop_v4
    pub fn multicast_loop_v4(&self) -> io::Result<bool> {
        self.io.get_ref().multicast_loop_v4()
    }

    /// Sets the value of the `IP_MULTICAST_LOOP` option for this socket.
    ///
    /// If enabled, multicast packets will be looped back to the local socket.
    ///
    /// # Note
    ///
    /// This may not have any affect on IPv6 sockets.
    pub fn set_multicast_loop_v4(&self, on: bool) -> io::Result<()> {
        self.io.get_ref().set_multicast_loop_v4(on)
    }

    /// Gets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// For more information about this option, see [`set_multicast_ttl_v4`].
    ///
    /// [`set_multicast_ttl_v4`]: method@Self::set_multicast_ttl_v4
    pub fn multicast_ttl_v4(&self) -> io::Result<u32> {
        self.io.get_ref().multicast_ttl_v4()
    }

    /// Sets the value of the `IP_MULTICAST_TTL` option for this socket.
    ///
    /// Indicates the time-to-live value of outgoing multicast packets for
    /// this socket. The default value is 1 which means that multicast packets
    /// don't leave the local network unless explicitly requested.
    ///
    /// # Note
    ///
    /// This may not have any affect on IPv6 sockets.
    pub fn set_multicast_ttl_v4(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_multicast_ttl_v4(ttl)
    }

    /// Gets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// For more information about this option, see [`set_multicast_loop_v6`].
    ///
    /// [`set_multicast_loop_v6`]: method@Self::set_multicast_loop_v6
    pub fn multicast_loop_v6(&self) -> io::Result<bool> {
        self.io.get_ref().multicast_loop_v6()
    }

    /// Sets the value of the `IPV6_MULTICAST_LOOP` option for this socket.
    ///
    /// Controls whether this socket sees the multicast packets it sends itself.
    ///
    /// # Note
    ///
    /// This may not have any affect on IPv4 sockets.
    pub fn set_multicast_loop_v6(&self, on: bool) -> io::Result<()> {
        self.io.get_ref().set_multicast_loop_v6(on)
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
    /// use tokio::net::UdpSocket;
    /// # use std::io;
    ///
    /// # async fn dox() -> io::Result<()> {
    /// let sock = UdpSocket::bind("127.0.0.1:8080").await?;
    ///
    /// println!("{:?}", sock.ttl()?);
    /// # Ok(())
    /// # }
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
    /// ```no_run
    /// use tokio::net::UdpSocket;
    /// # use std::io;
    ///
    /// # async fn dox() -> io::Result<()> {
    /// let sock = UdpSocket::bind("127.0.0.1:8080").await?;
    /// sock.set_ttl(60)?;
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn set_ttl(&self, ttl: u32) -> io::Result<()> {
        self.io.get_ref().set_ttl(ttl)
    }

    /// Executes an operation of the `IP_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// address of the local interface with which the system should join the
    /// multicast group. If it's equal to `INADDR_ANY` then an appropriate
    /// interface is chosen by the system.
    pub fn join_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.io.get_ref().join_multicast_v4(&multiaddr, &interface)
    }

    /// Executes an operation of the `IPV6_ADD_MEMBERSHIP` type.
    ///
    /// This function specifies a new multicast group for this socket to join.
    /// The address must be a valid multicast address, and `interface` is the
    /// index of the interface to join/leave (or 0 to indicate any interface).
    pub fn join_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.io.get_ref().join_multicast_v6(multiaddr, interface)
    }

    /// Executes an operation of the `IP_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`join_multicast_v4`].
    ///
    /// [`join_multicast_v4`]: method@Self::join_multicast_v4
    pub fn leave_multicast_v4(&self, multiaddr: Ipv4Addr, interface: Ipv4Addr) -> io::Result<()> {
        self.io.get_ref().leave_multicast_v4(&multiaddr, &interface)
    }

    /// Executes an operation of the `IPV6_DROP_MEMBERSHIP` type.
    ///
    /// For more information about this option, see [`join_multicast_v6`].
    ///
    /// [`join_multicast_v6`]: method@Self::join_multicast_v6
    pub fn leave_multicast_v6(&self, multiaddr: &Ipv6Addr, interface: u32) -> io::Result<()> {
        self.io.get_ref().leave_multicast_v6(multiaddr, interface)
    }

    /// Returns the value of the `SO_ERROR` option.
    ///
    /// # Examples
    /// ```
    /// # use std::error::Error;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn Error>> {
    /// use tokio::net::UdpSocket;
    ///
    /// // Create a socket
    /// let socket = UdpSocket::bind("0.0.0.0:8080").await?;
    ///
    /// if let Ok(Some(err)) = socket.take_error() {
    ///     println!("Got error: {:?}", err);
    /// }
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }
}

impl TryFrom<std::net::UdpSocket> for UdpSocket {
    type Error = io::Error;

    /// Consumes stream, returning the tokio I/O object.
    ///
    /// This is equivalent to
    /// [`UdpSocket::from_std(stream)`](UdpSocket::from_std).
    fn try_from(stream: std::net::UdpSocket) -> Result<Self, Self::Error> {
        Self::from_std(stream)
    }
}

impl fmt::Debug for UdpSocket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

#[cfg(all(unix))]
mod sys {
    use super::UdpSocket;
    use std::os::unix::prelude::*;

    impl AsRawFd for UdpSocket {
        fn as_raw_fd(&self) -> RawFd {
            self.io.get_ref().as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    use super::UdpSocket;
    use std::os::windows::prelude::*;

    impl AsRawSocket for UdpSocket {
        fn as_raw_socket(&self) -> RawSocket {
            self.io.get_ref().as_raw_socket()
        }
    }
}
