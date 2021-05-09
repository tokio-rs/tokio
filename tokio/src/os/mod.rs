//! Platform-specific utilities used by Tokio.
//!
//! **Note** this should not be used directly. It is only present for
//! documentation purposes.

// Documentation copied from https://github.com/rust-lang/rust under the MIT license.
// See: https://github.com/rust-lang/rust/blob/master/LICENSE-MIT

/// Platform-specific extensions to std for Unix platforms.
#[cfg_attr(docsrs, doc(cfg(unix)))]
#[cfg(not(unix))]
#[allow(missing_debug_implementations)]
#[allow(unused)]
pub mod unix {
    /// Unix-specific extensions to general I/O primitives.
    pub mod io {
        /// Raw file descriptors.
        pub struct RawFd(());

        /// A trait to extract the raw unix file descriptor from an underlying
        /// object.
        ///
        /// This is only available on unix platforms and must be imported in order
        /// to call the method. Windows platforms have a corresponding `AsRawHandle`
        /// and `AsRawSocket` set of traits.
        pub trait AsRawFd {
            /// Extracts the raw file descriptor.
            ///
            /// This method does **not** pass ownership of the raw file descriptor
            /// to the caller. The descriptor is only guaranteed to be valid while
            /// the original object has not yet been destroyed.
            ///
            /// # Example
            ///
            /// ```no_run
            /// use std::fs::File;
            /// # use std::io;
            /// use std::os::unix::io::{AsRawFd, RawFd};
            ///
            /// let mut f = File::open("foo.txt")?;
            /// // Note that `raw_fd` is only valid as long as `f` exists.
            /// let raw_fd: RawFd = f.as_raw_fd();
            /// # Ok::<(), io::Error>(())
            /// ```
            fn as_raw_fd(&self) -> RawFd;
        }
    }

    /// Unix-specific networking functionality
    pub mod net {
        use std::io;

        /// An address associated with a Unix socket.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use std::os::unix::net::UnixListener;
        ///
        /// let socket = match UnixListener::bind("/tmp/sock") {
        ///     Ok(sock) => sock,
        ///     Err(e) => {
        ///         println!("Couldn't bind: {:?}", e);
        ///         return
        ///     }
        /// };
        /// let addr = socket.local_addr().expect("Couldn't get local address");
        /// ```
        pub struct SocketAddr(());

        ///
        /// # Examples
        ///
        /// ```no_run
        /// use std::os::unix::net::UnixDatagram;
        ///
        /// fn main() -> std::io::Result<()> {
        ///     let socket = UnixDatagram::bind("/path/to/my/socket")?;
        ///     socket.send_to(b"hello world", "/path/to/other/socket")?;
        ///     let mut buf = [0; 100];
        ///     let (count, address) = socket.recv_from(&mut buf)?;
        ///     println!("socket {:?} sent {:?}", address, &buf[..count]);
        ///     Ok(())
        /// }
        /// ```
        pub struct UnixDatagram(());

        impl UnixDatagram {
            /// Moves the socket into or out of nonblocking mode.
            ///
            /// # Examples
            ///
            /// ```no_run
            /// use std::os::unix::net::UnixDatagram;
            ///
            /// fn main() -> std::io::Result<()> {
            ///     let sock = UnixDatagram::unbound()?;
            ///     sock.set_nonblocking(true).expect("set_nonblocking function failed");
            ///     Ok(())
            /// }
            /// ```
            pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
                unimplemented!()
            }
        }

        /// A Unix stream socket.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use std::os::unix::net::UnixStream;
        /// use std::io::prelude::*;
        ///
        /// fn main() -> std::io::Result<()> {
        ///     let mut stream = UnixStream::connect("/path/to/my/socket")?;
        ///     stream.write_all(b"hello world")?;
        ///     let mut response = String::new();
        ///     stream.read_to_string(&mut response)?;
        ///     println!("{}", response);
        ///     Ok(())
        /// }
        /// ```
        pub struct UnixStream(());

        impl UnixStream {
            /// Moves the socket into or out of nonblocking mode.
            ///
            /// # Examples
            ///
            /// ```no_run
            /// use std::os::unix::net::UnixStream;
            ///
            /// fn main() -> std::io::Result<()> {
            ///     let socket = UnixStream::connect("/tmp/sock")?;
            ///     socket.set_nonblocking(true).expect("Couldn't set nonblocking");
            ///     Ok(())
            /// }
            /// ```
            pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
                unimplemented!()
            }
        }

        /// A structure representing a Unix domain socket server.
        ///
        /// # Examples
        ///
        /// ```no_run
        /// use std::thread;
        /// use std::os::unix::net::{UnixStream, UnixListener};
        ///
        /// fn handle_client(stream: UnixStream) {
        ///     // ...
        /// }
        ///
        /// fn main() -> std::io::Result<()> {
        ///     let listener = UnixListener::bind("/path/to/the/socket")?;
        ///
        ///     // accept connections and process them, spawning a new thread for each one
        ///     for stream in listener.incoming() {
        ///         match stream {
        ///             Ok(stream) => {
        ///                 /* connection succeeded */
        ///                 thread::spawn(|| handle_client(stream));
        ///             }
        ///             Err(err) => {
        ///                 /* connection failed */
        ///                 break;
        ///             }
        ///         }
        ///     }
        ///     Ok(())
        /// }
        /// ```
        pub struct UnixListener(());

        impl UnixListener {
            /// Moves the socket into or out of nonblocking mode.
            ///
            /// This will result in the `accept` operation becoming nonblocking,
            /// i.e., immediately returning from their calls. If the IO operation is
            /// successful, `Ok` is returned and no further action is required. If the
            /// IO operation could not be completed and needs to be retried, an error
            /// with kind [`io::ErrorKind::WouldBlock`] is returned.
            ///
            /// # Examples
            ///
            /// ```no_run
            /// use std::os::unix::net::UnixListener;
            ///
            /// fn main() -> std::io::Result<()> {
            ///     let listener = UnixListener::bind("/path/to/the/socket")?;
            ///     listener.set_nonblocking(true).expect("Couldn't set non blocking");
            ///     Ok(())
            /// }
            /// ```
            pub fn set_nonblocking(&self, nonblocking: bool) -> io::Result<()> {
                unimplemented!()
            }
        }
    }
}

#[cfg(unix)]
#[allow(unused)]
pub(crate) mod unix {
    pub(crate) use std::os::unix::{io, net};
}
