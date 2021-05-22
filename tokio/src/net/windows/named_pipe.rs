use std::ffi::c_void;
use std::ffi::OsStr;
use std::io;
use std::mem;
use std::pin::Pin;
use std::ptr;
use std::task::{Context, Poll};

use crate::io::{AsyncRead, AsyncWrite, Interest, PollEvented, ReadBuf};
use crate::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};

// Hide imports which are not used when generating documentation.
#[cfg(not(docsrs))]
mod doc {
    pub(super) use crate::os::windows::ffi::OsStrExt;
    pub(super) use crate::winapi::shared::minwindef::{DWORD, FALSE};
    pub(super) use crate::winapi::um::fileapi;
    pub(super) use crate::winapi::um::handleapi;
    pub(super) use crate::winapi::um::namedpipeapi;
    pub(super) use crate::winapi::um::winbase;
    pub(super) use crate::winapi::um::winnt;

    pub(super) use mio::windows as mio_windows;
}

// NB: none of these shows up in public API, so don't document them.
#[cfg(docsrs)]
mod doc {
    pub type DWORD = crate::doc::NotDefinedHere;

    pub(super) mod mio_windows {
        pub type NamedPipe = crate::doc::NotDefinedHere;
    }
}

use self::doc::*;

/// A [Windows named pipe].
///
/// Constructed using [`NamedPipeClientOptions::create`] for clients, or
/// [`NamedPipeOptions::create`] for servers. See their corresponding
/// documentation for examples.
///
/// Connecting a client involves a few steps. First we must try to [`create`], the
/// error typically indicates one of two things:
///
/// * [`std::io::ErrorKind::NotFound`] - There is no server available.
/// * [`ERROR_PIPE_BUSY`] - There is a server available, but it is busy. Sleep for
///   a while and try again.
///
/// So a typical client connect loop will look like the this:
///
/// ```no_run
/// use std::time::Duration;
/// use tokio::net::windows::NamedPipeClientOptions;
/// use tokio::time;
/// use winapi::shared::winerror;
///
/// const PIPE_NAME: &str = r"\\.\pipe\named-pipe-idiomatic-client";
///
/// # #[tokio::main] async fn main() -> std::io::Result<()> {
/// let client = loop {
///     match NamedPipeClientOptions::new().create(PIPE_NAME) {
///         Ok(client) => break client,
///         Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
///         Err(e) => return Err(e),
///     }
///
///     time::sleep(Duration::from_millis(50)).await;
/// };
///
/// /* use the connected client */
/// # Ok(()) }
/// ```
///
/// A client will error with [`std::io::ErrorKind::NotFound`] for most creation
/// oriented operations like [`create`] unless at least once server instance is up
/// and running at all time. This means that the typical listen loop for a
/// server is a bit involved, because we have to ensure that we never drop a
/// server accidentally while a client might want to connect.
///
/// ```no_run
/// use std::io;
/// use tokio::net::windows::NamedPipeOptions;
///
/// const PIPE_NAME: &str = r"\\.\pipe\named-pipe-idiomatic-server";
///
/// # #[tokio::main] async fn main() -> std::io::Result<()> {
/// // The first server needs to be constructed early so that clients can
/// // be correctly connected. Otherwise calling .wait will cause the client to
/// // error.
/// //
/// // Here we also make use of `first_pipe_instance`, which will ensure that
/// // there are no other servers up and running already.
/// let mut server = NamedPipeOptions::new()
///     .first_pipe_instance(true)
///     .create(PIPE_NAME)?;
///
/// // Spawn the server loop.
/// let server = tokio::spawn(async move {
///     loop {
///         // Wait for a client to connect.
///         let connected = server.connect().await?;
///
///         // Construct the next server to be connected before sending the one
///         // we already have of onto a task. This ensures that the server
///         // isn't closed (after it's done in the task) before a new one is
///         // available. Otherwise the client might error with
///         // `io::ErrorKind::NotFound`.
///         server = NamedPipeOptions::new().create(PIPE_NAME)?;
///
///         let client = tokio::spawn(async move {
///             /* use the connected client */
/// #           Ok::<_, std::io::Error>(())
///         });
/// #       if true { break } // needed for type inference to work
///     }
///
///     Ok::<_, io::Error>(())
/// });
///
/// /* do something else not server related here */
/// # Ok(()) }
/// ```
///
/// [`create`]: NamedPipeClientOptions::create
/// [`ERROR_PIPE_BUSY`]: crate::winapi::shared::winerror::ERROR_PIPE_BUSY
/// [Windows named pipe]: https://docs.microsoft.com/en-us/windows/win32/ipc/named-pipes
#[derive(Debug)]
pub struct NamedPipe {
    io: PollEvented<mio_windows::NamedPipe>,
}

impl NamedPipe {
    /// Fallibly construct a new named pipe from the specified raw handle.
    ///
    /// This function will consume ownership of the handle given, passing
    /// responsibility for closing the handle to the returned object.
    ///
    /// This function is also unsafe as the primitives currently returned have
    /// the contract that they are the sole owner of the file descriptor they
    /// are wrapping. Usage of this function could accidentally allow violating
    /// this contract which can cause memory unsafety in code that relies on it
    /// being true.
    ///
    /// # Errors
    ///
    /// This errors if called outside of a [Tokio Runtime] which doesn't have
    /// [I/O enabled].
    ///
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [I/O enabled]: crate::runtime::Builder::enable_io
    pub unsafe fn try_from_raw_handle(handle: RawHandle) -> io::Result<Self> {
        let named_pipe = mio_windows::NamedPipe::from_raw_handle(handle);

        Ok(NamedPipe {
            io: PollEvented::new(named_pipe)?,
        })
    }

    /// Enables a named pipe server process to wait for a client process to
    /// connect to an instance of a named pipe. A client process connects by
    /// creating a named pipe with the same name.
    ///
    /// This corresponds to the [`ConnectNamedPipe`] system call.
    ///
    /// [`ConnectNamedPipe`]: https://docs.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe
    ///
    /// ```no_run
    /// use tokio::net::windows::NamedPipeOptions;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\mynamedpipe";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let pipe = NamedPipeOptions::new().create(PIPE_NAME)?;
    ///
    /// // Wait for a client to connect.
    /// pipe.connect().await?;
    ///
    /// // Use the connected client...
    /// # Ok(()) }
    /// ```
    pub async fn connect(&self) -> io::Result<()> {
        loop {
            match self.io.connect() {
                Ok(()) => break,
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    self.io.registration().readiness(Interest::WRITABLE).await?;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(())
    }

    /// Disconnects the server end of a named pipe instance from a client
    /// process.
    ///
    /// ```
    /// use tokio::io::AsyncWriteExt as _;
    /// use tokio::net::windows::{NamedPipeOptions, NamedPipeClientOptions};
    /// use winapi::shared::winerror;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-disconnect";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server = NamedPipeOptions::new().create(PIPE_NAME)?;
    ///
    /// let mut client = NamedPipeClientOptions::new()
    ///     .create(PIPE_NAME)?;
    ///
    /// // Wait for a client to become connected.
    /// server.connect().await?;
    ///
    /// // Forcibly disconnect the client.
    /// server.disconnect()?;
    ///
    /// // Write fails with an OS-specific error after client has been
    /// // disconnected.
    /// let e = client.write(b"ping").await.unwrap_err();
    /// assert_eq!(e.raw_os_error(), Some(winerror::ERROR_PIPE_NOT_CONNECTED as i32));
    /// # Ok(()) }
    /// ```
    pub fn disconnect(&self) -> io::Result<()> {
        self.io.disconnect()
    }

    /// Retrieves information about the current named pipe.
    ///
    /// ```
    /// use tokio::net::windows::{NamedPipeOptions, NamedPipeClientOptions, PipeMode, PipeEnd};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-info";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server = NamedPipeOptions::new()
    ///     .pipe_mode(PipeMode::Message)
    ///     .max_instances(5)
    ///     .create(PIPE_NAME)?;
    ///
    /// let client = NamedPipeClientOptions::new()
    ///     .create(PIPE_NAME)?;
    ///
    /// let server_info = server.info()?;
    /// let client_info = client.info()?;
    ///
    /// assert_eq!(server_info.end, PipeEnd::Server);
    /// assert_eq!(server_info.mode, PipeMode::Message);
    /// assert_eq!(server_info.max_instances, 5);
    ///
    /// assert_eq!(client_info.end, PipeEnd::Client);
    /// assert_eq!(client_info.mode, PipeMode::Message);
    /// assert_eq!(server_info.max_instances, 5);
    /// # Ok(()) }
    /// ```
    pub fn info(&self) -> io::Result<PipeInfo> {
        unsafe {
            let mut flags = mem::MaybeUninit::uninit();
            let mut out_buffer_size = mem::MaybeUninit::uninit();
            let mut in_buffer_size = mem::MaybeUninit::uninit();
            let mut max_instances = mem::MaybeUninit::uninit();

            let result = namedpipeapi::GetNamedPipeInfo(
                self.io.as_raw_handle(),
                flags.as_mut_ptr(),
                out_buffer_size.as_mut_ptr(),
                in_buffer_size.as_mut_ptr(),
                max_instances.as_mut_ptr(),
            );

            if result == FALSE {
                return Err(io::Error::last_os_error());
            }

            let flags = flags.assume_init();
            let out_buffer_size = out_buffer_size.assume_init();
            let in_buffer_size = in_buffer_size.assume_init();
            let max_instances = max_instances.assume_init();

            let mut end = PipeEnd::Client;
            let mut mode = PipeMode::Byte;

            if flags & winbase::PIPE_SERVER_END != 0 {
                end = PipeEnd::Server;
            }

            if flags & winbase::PIPE_TYPE_MESSAGE != 0 {
                mode = PipeMode::Message;
            }

            Ok(PipeInfo {
                end,
                mode,
                out_buffer_size,
                in_buffer_size,
                max_instances,
            })
        }
    }

    /// Copies data from a named or anonymous pipe into a buffer without
    /// removing it from the pipe. It also returns information about data in the
    /// pipe.
    ///
    /// # Considerations
    ///
    /// Data reported through peek is sporadic. Once peek returns any data for a
    /// given named pipe, further calls to it are not gauranteed to return the
    /// same or higher number of bytes available ([`total_bytes_available`]). It
    /// might even report a count of `0` even if no data has been read from the
    /// named pipe that was previously peeked.
    ///
    /// Peeking does not update the state of the named pipe, so in order to
    /// advance it you have to actively issue reads. A peek reporting a number
    /// of bytes available ([`total_bytes_available`]) of `0` does not guarantee
    /// that there is no data available to read from the named pipe. Even if a
    /// peer is writing data, reads still have to be issued for the state of the
    /// named pipe to update.
    ///
    /// Finally, peeking might report no data available indefinitely if there's
    /// too little data in the buffer of the named pipe.
    ///
    /// You can play around with the [`named-pipe-peek` example] to get a feel
    /// for how this function behaves.
    ///
    /// [`total_bytes_available`]: PipePeekInfo::total_bytes_available
    /// [`named-pipe-peek` example]: https://github.com/tokio-rs/tokio/blob/master/examples/named-pipe-peek.rs
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    /// use tokio::net::windows::{NamedPipeOptions, NamedPipeClientOptions};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-peek-consumed";
    /// const N: usize = 100;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let mut server = NamedPipeOptions::new().create(PIPE_NAME)?;
    /// let mut client = NamedPipeClientOptions::new().create(PIPE_NAME)?;
    /// server.connect().await?;
    ///
    /// let client = tokio::spawn(async move {
    ///     for _ in 0..N {
    ///         client.write_all(b"ping").await?;
    ///     }
    ///
    ///     let mut buf = [0u8; 4];
    ///     client.read_exact(&mut buf).await?;
    ///
    ///     Ok::<_, io::Error>(buf)
    /// });
    ///
    /// let mut buf = [0u8; 4];
    /// let mut available = 0;
    ///
    /// for n in 0..N {
    ///     if available < 4 {
    ///         server.read_exact(&mut buf).await?;
    ///         assert_eq!(&buf[..], b"ping");
    ///
    ///         let info = server.peek(None)?;
    ///         available = info.total_bytes_available;
    ///         continue;
    ///     }
    ///
    ///     // here we know that at least `available` bytes are immediately
    ///     // ready to read.
    ///     server.read_exact(&mut buf).await?;
    ///     available -= buf.len();
    ///     assert_eq!(&buf[..], b"ping");
    /// }
    ///
    /// server.write_all(b"pong").await?;
    ///
    /// let buf = client.await??;
    /// assert_eq!(&buf[..], b"pong");
    /// # Ok(()) }
    /// ```
    pub fn peek(&mut self, mut buf: Option<&mut ReadBuf<'_>>) -> io::Result<PipePeekInfo> {
        use std::convert::TryFrom as _;

        unsafe {
            let mut n = 0;
            let mut total_bytes_available = 0;
            let mut bytes_left_this_message = 0;

            let result = {
                let (buf, len) = match &mut buf {
                    Some(buf) => {
                        let unfilled = buf.unfilled_mut();
                        let len = <DWORD>::try_from(unfilled.len()).unwrap_or(DWORD::MAX);
                        // NB: The OS has no expectation on whether the buffer
                        // is initialized or not.
                        (unfilled.as_mut_ptr() as *mut _, len)
                    }
                    None => (ptr::null_mut(), 0),
                };

                namedpipeapi::PeekNamedPipe(
                    self.io.as_raw_handle(),
                    buf,
                    len,
                    &mut n,
                    &mut total_bytes_available,
                    &mut bytes_left_this_message,
                )
            };

            if result == FALSE {
                return Err(io::Error::last_os_error());
            }

            // NB: This is either guaranteed to fit within `usize` because of
            // the clamping above, or the OS is reporting bogus values.
            let n = usize::try_from(n).unwrap();

            if let Some(buf) = buf {
                // Safety: we trust that the OS has initialized up until `n`
                // through the call to PeekNamedPipe.
                buf.assume_init(n);
                buf.advance(n);
            }

            let total_bytes_available =
                usize::try_from(total_bytes_available).expect("available bytes too large");
            let bytes_left_this_message =
                usize::try_from(bytes_left_this_message).expect("bytes left in message too large");

            let info = PipePeekInfo {
                total_bytes_available,
                bytes_left_this_message,
            };

            Ok(info)
        }
    }
}

impl AsyncRead for NamedPipe {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        unsafe { self.io.poll_read(cx, buf) }
    }
}

impl AsyncWrite for NamedPipe {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.io.poll_write_vectored(cx, bufs)
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_flush(cx)
    }
}

impl AsRawHandle for NamedPipe {
    fn as_raw_handle(&self) -> RawHandle {
        self.io.as_raw_handle()
    }
}

// Helper to set a boolean flag as a bitfield.
macro_rules! bool_flag {
    ($f:expr, $t:expr, $flag:expr) => {{
        let current = $f;

        if $t {
            $f = current | $flag;
        } else {
            $f = current & !$flag;
        };
    }};
}

/// A builder structure for construct a named pipe with named pipe-specific
/// options. This is required to use for named pipe servers who wants to modify
/// pipe-related options.
///
/// See [`NamedPipeOptions::create`].
#[derive(Debug, Clone)]
pub struct NamedPipeOptions {
    open_mode: DWORD,
    pipe_mode: DWORD,
    max_instances: DWORD,
    out_buffer_size: DWORD,
    in_buffer_size: DWORD,
    default_timeout: DWORD,
}

impl NamedPipeOptions {
    /// Creates a new named pipe builder with the default settings.
    ///
    /// ```
    /// use tokio::net::windows::NamedPipeOptions;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-new";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server = NamedPipeOptions::new()
    ///     .create(PIPE_NAME)?;
    /// # Ok(()) }
    /// ```
    pub fn new() -> NamedPipeOptions {
        NamedPipeOptions {
            open_mode: winbase::PIPE_ACCESS_DUPLEX | winbase::FILE_FLAG_OVERLAPPED,
            pipe_mode: winbase::PIPE_TYPE_BYTE | winbase::PIPE_REJECT_REMOTE_CLIENTS,
            max_instances: winbase::PIPE_UNLIMITED_INSTANCES,
            out_buffer_size: 65536,
            in_buffer_size: 65536,
            default_timeout: 0,
        }
    }

    /// The pipe mode.
    ///
    /// The default pipe mode is [`PipeMode::Byte`]. See [`PipeMode`] for
    /// documentation of what each mode means.
    ///
    /// This corresponding to specifying [`dwPipeMode`].
    ///
    /// [`dwPipeMode`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    pub fn pipe_mode(&mut self, pipe_mode: PipeMode) -> &mut Self {
        self.pipe_mode = match pipe_mode {
            PipeMode::Byte => winbase::PIPE_TYPE_BYTE,
            PipeMode::Message => winbase::PIPE_TYPE_MESSAGE,
        };

        self
    }

    /// The flow of data in the pipe goes from client to server only.
    ///
    /// This corresponds to setting [`PIPE_ACCESS_INBOUND`].
    ///
    /// [`PIPE_ACCESS_INBOUND`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_access_inbound
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    /// use tokio::net::windows::{NamedPipeClientOptions, NamedPipeOptions};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-access-inbound";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// // Server side prevents connecting by denying inbound access, client errors
    /// // when attempting to create the connection.
    /// {
    ///     let _server = NamedPipeOptions::new()
    ///         .access_inbound(false)
    ///         .create(PIPE_NAME)?;
    ///
    ///     let e = NamedPipeClientOptions::new()
    ///         .create(PIPE_NAME)
    ///         .unwrap_err();
    ///
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    ///
    ///     // Disabling writing allows a client to connect, but leads to runtime
    ///     // error if a write is attempted.
    ///     let mut client = NamedPipeClientOptions::new()
    ///         .write(false)
    ///         .create(PIPE_NAME)?;
    ///
    ///     let e = client.write(b"ping").await.unwrap_err();
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    /// }
    ///
    /// // A functional, unidirectional server-to-client only communication.
    /// {
    ///     let mut server = NamedPipeOptions::new()
    ///         .access_inbound(false)
    ///         .create(PIPE_NAME)?;
    ///
    ///     let mut client = NamedPipeClientOptions::new()
    ///         .write(false)
    ///         .create(PIPE_NAME)?;
    ///
    ///     let write = server.write_all(b"ping");
    ///
    ///     let mut buf = [0u8; 4];
    ///     let read = client.read_exact(&mut buf);
    ///
    ///     let ((), read) = tokio::try_join!(write, read)?;
    ///
    ///     assert_eq!(read, 4);
    ///     assert_eq!(&buf[..], b"ping");
    /// }
    /// # Ok(()) }
    /// ```
    pub fn access_inbound(&mut self, allowed: bool) -> &mut Self {
        bool_flag!(self.open_mode, allowed, winbase::PIPE_ACCESS_INBOUND);
        self
    }

    /// The flow of data in the pipe goes from server to client only.
    ///
    /// This corresponds to setting [`PIPE_ACCESS_OUTBOUND`].
    ///
    /// [`PIPE_ACCESS_OUTBOUND`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_access_outbound
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    /// use tokio::net::windows::{NamedPipeClientOptions, NamedPipeOptions};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-access-outbound";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// // Server side prevents connecting by denying outbound access, client errors
    /// // when attempting to create the connection.
    /// {
    ///     let _server = NamedPipeOptions::new()
    ///         .access_outbound(false)
    ///         .create(PIPE_NAME)?;
    ///
    ///     let e = NamedPipeClientOptions::new()
    ///         .create(PIPE_NAME)
    ///         .unwrap_err();
    ///
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    ///
    ///     // Disabling reading allows a client to connect, but leads to runtime
    ///     // error if a read is attempted.
    ///     let mut client = NamedPipeClientOptions::new().read(false).create(PIPE_NAME)?;
    ///
    ///     let mut buf = [0u8; 4];
    ///     let e = client.read(&mut buf).await.unwrap_err();
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    /// }
    ///
    /// // A functional, unidirectional client-to-server only communication.
    /// {
    ///     let mut server = NamedPipeOptions::new().access_outbound(false).create(PIPE_NAME)?;
    ///     let mut client = NamedPipeClientOptions::new().read(false).create(PIPE_NAME)?;
    ///
    ///     // TODO: Explain why this test doesn't work without calling connect
    ///     // first.
    ///     //
    ///     // Because I have no idea -- udoprog
    ///     server.connect().await?;
    ///
    ///     let write = client.write_all(b"ping");
    ///
    ///     let mut buf = [0u8; 4];
    ///     let read = server.read_exact(&mut buf);
    ///
    ///     let ((), read) = tokio::try_join!(write, read)?;
    ///
    ///     println!("done reading and writing");
    ///
    ///     assert_eq!(read, 4);
    ///     assert_eq!(&buf[..], b"ping");
    /// }
    /// # Ok(()) }
    /// ```
    pub fn access_outbound(&mut self, allowed: bool) -> &mut Self {
        bool_flag!(self.open_mode, allowed, winbase::PIPE_ACCESS_OUTBOUND);
        self
    }

    /// If you attempt to create multiple instances of a pipe with this flag,
    /// creation of the first instance succeeds, but creation of the next
    /// instance fails with [`ERROR_ACCESS_DENIED`].
    ///
    /// This corresponds to setting [`FILE_FLAG_FIRST_PIPE_INSTANCE`].
    ///
    /// [`ERROR_ACCESS_DENIED`]: crate::winapi::shared::winerror::ERROR_ACCESS_DENIED
    /// [`FILE_FLAG_FIRST_PIPE_INSTANCE`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_first_pipe_instance
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::net::windows::NamedPipeOptions;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-first-instance";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// let mut builder = NamedPipeOptions::new();
    /// builder.first_pipe_instance(true);
    ///
    /// let server = builder.create(PIPE_NAME)?;
    /// let e = builder.create(PIPE_NAME).unwrap_err();
    /// assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    /// drop(server);
    ///
    /// // OK: since, we've closed the other instance.
    /// let _server2 = builder.create(PIPE_NAME)?;
    /// # Ok(()) }
    /// ```
    pub fn first_pipe_instance(&mut self, first: bool) -> &mut Self {
        bool_flag!(
            self.open_mode,
            first,
            winbase::FILE_FLAG_FIRST_PIPE_INSTANCE
        );
        self
    }

    /// Indicates whether this server can accept remote clients or not. This is
    /// enabled by default.
    ///
    /// This corresponds to setting [`PIPE_REJECT_REMOTE_CLIENTS`].
    ///
    /// [`PIPE_REJECT_REMOTE_CLIENTS`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_reject_remote_clients
    pub fn reject_remote_clients(&mut self, reject: bool) -> &mut Self {
        bool_flag!(self.pipe_mode, reject, winbase::PIPE_REJECT_REMOTE_CLIENTS);
        self
    }

    /// The maximum number of instances that can be created for this pipe. The
    /// first instance of the pipe can specify this value; the same number must
    /// be specified for other instances of the pipe. Acceptable values are in
    /// the range 1 through 254. The default value is unlimited.
    ///
    /// This value is only respected by the first instance that creates the pipe
    /// and is ignored otherwise.
    ///
    /// This corresponds to specifying [`nMaxInstances`].
    ///
    /// [`nMaxInstances`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    ///
    /// # Errors
    ///
    /// The same numbers of `max_instances` have to be used by all servers. Any
    /// additional servers trying to be built which uses a mismatching value
    /// will error.
    ///
    /// ```
    /// use std::io;
    /// use tokio::net::windows::{NamedPipeOptions, NamedPipeClientOptions};
    /// use winapi::shared::winerror;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-max-instances";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// let mut server = NamedPipeOptions::new();
    /// server.max_instances(2);
    ///
    /// let s1 = server.create(PIPE_NAME)?;
    /// let c1 = NamedPipeClientOptions::new().create(PIPE_NAME);
    ///
    /// let s2 = server.create(PIPE_NAME)?;
    /// let c2 = NamedPipeClientOptions::new().create(PIPE_NAME);
    ///
    /// // Too many servers!
    /// let e = server.create(PIPE_NAME).unwrap_err();
    /// assert_eq!(e.raw_os_error(), Some(winerror::ERROR_PIPE_BUSY as i32));
    ///
    /// // Still too many servers even if we specify a higher value!
    /// let e = server.max_instances(100).create(PIPE_NAME).unwrap_err();
    /// assert_eq!(e.raw_os_error(), Some(winerror::ERROR_PIPE_BUSY as i32));
    /// # Ok(()) }
    /// ```
    ///
    /// # Panics
    ///
    /// This function will panic if more than 254 instances are specified. If
    /// you do not wish to set an instance limit, leave it unspecified.
    ///
    /// ```should_panic
    /// use tokio::net::windows::NamedPipeOptions;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeOptions::new().max_instances(255);
    /// # Ok(()) }
    /// ```
    pub fn max_instances(&mut self, instances: usize) -> &mut Self {
        assert!(instances < 255, "cannot specify more than 254 instances");
        self.max_instances = instances as DWORD;
        self
    }

    /// The number of bytes to reserve for the output buffer.
    ///
    /// This corresponds to specifying [`nOutBufferSize`].
    ///
    /// [`nOutBufferSize`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    pub fn out_buffer_size(&mut self, buffer: u32) -> &mut Self {
        self.out_buffer_size = buffer as DWORD;
        self
    }

    /// The number of bytes to reserve for the input buffer.
    ///
    /// This corresponds to specifying [`nInBufferSize`].
    ///
    /// [`nInBufferSize`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    pub fn in_buffer_size(&mut self, buffer: u32) -> &mut Self {
        self.in_buffer_size = buffer as DWORD;
        self
    }

    /// Create the named pipe identified by `addr` for use as a server.
    ///
    /// This function will call the [`CreateNamedPipe`] function and return the
    /// result.
    ///
    /// [`CreateNamedPipe`]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    ///
    /// # Errors
    ///
    /// This errors if called outside of a [Tokio Runtime] which doesn't have
    /// [I/O enabled] or if any OS-specific I/O errors occur.
    ///
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [I/O enabled]: crate::runtime::Builder::enable_io
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::windows::NamedPipeOptions;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-create";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server = NamedPipeOptions::new().create(PIPE_NAME)?;
    /// # Ok(()) }
    /// ```
    pub fn create(&self, addr: impl AsRef<OsStr>) -> io::Result<NamedPipe> {
        // Safety: We're calling create_with_security_attributes_raw w/ a null
        // pointer which disables it.
        unsafe { self.create_with_security_attributes_raw(addr, ptr::null_mut()) }
    }

    /// Create the named pipe identified by `addr` for use as a server.
    ///
    /// This is the same as [`create`] except that it supports providing the raw
    /// pointer to a structure of [`SECURITY_ATTRIBUTES`] which will be passed
    /// as the `lpSecurityAttributes` argument to [`CreateFile`].
    ///
    /// # Errors
    ///
    /// This errors if called outside of a [Tokio Runtime] which doesn't have
    /// [I/O enabled] or if any OS-specific I/O errors occur.
    ///
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [I/O enabled]: crate::runtime::Builder::enable_io
    ///
    /// # Safety
    ///
    /// The `attrs` argument must either be null or point at a valid instance of
    /// the [`SECURITY_ATTRIBUTES`] structure. If the argument is null, the
    /// behavior is identical to calling the [`create`] method.
    ///
    /// [`create`]: NamedPipeOptions::create
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
    /// [`SECURITY_ATTRIBUTES`]: crate::winapi::um::minwinbase::SECURITY_ATTRIBUTES
    pub unsafe fn create_with_security_attributes_raw(
        &self,
        addr: impl AsRef<OsStr>,
        attrs: *mut c_void,
    ) -> io::Result<NamedPipe> {
        let addr = encode_addr(addr);

        let h = namedpipeapi::CreateNamedPipeW(
            addr.as_ptr(),
            self.open_mode,
            self.pipe_mode,
            self.max_instances,
            self.out_buffer_size,
            self.in_buffer_size,
            self.default_timeout,
            attrs as *mut _,
        );

        if h == handleapi::INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let io = mio_windows::NamedPipe::from_raw_handle(h);
        let io = PollEvented::new(io)?;

        Ok(NamedPipe { io })
    }
}

/// A builder suitable for building and interacting with named pipes from the
/// client side.
///
/// See [`NamedPipeClientOptions::create`].
#[derive(Debug, Clone)]
pub struct NamedPipeClientOptions {
    desired_access: DWORD,
    security_qos_flags: DWORD,
}

impl NamedPipeClientOptions {
    /// Creates a new named pipe builder with the default settings.
    ///
    /// ```
    /// use tokio::net::windows::{NamedPipeOptions, NamedPipeClientOptions};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-client-new";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// // Server must be created in order for the client creation to succeed.
    /// let server = NamedPipeOptions::new().create(PIPE_NAME)?;
    /// let client = NamedPipeClientOptions::new().create(PIPE_NAME)?;
    /// # Ok(()) }
    /// ```
    pub fn new() -> Self {
        Self {
            desired_access: winnt::GENERIC_READ | winnt::GENERIC_WRITE,
            security_qos_flags: winbase::SECURITY_IDENTIFICATION | winbase::SECURITY_SQOS_PRESENT,
        }
    }

    /// If the client supports reading data. This is enabled by default.
    ///
    /// This corresponds to setting [`GENERIC_READ`] in the call to [`CreateFile`].
    ///
    /// [`GENERIC_READ`]: https://docs.microsoft.com/en-us/windows/win32/secauthz/generic-access-rights
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
    pub fn read(&mut self, allowed: bool) -> &mut Self {
        bool_flag!(self.desired_access, allowed, winnt::GENERIC_READ);
        self
    }

    /// If the created pipe supports writing data. This is enabled by default.
    ///
    /// This corresponds to setting [`GENERIC_WRITE`] in the call to [`CreateFile`].
    ///
    /// [`GENERIC_WRITE`]: https://docs.microsoft.com/en-us/windows/win32/secauthz/generic-access-rights
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
    pub fn write(&mut self, allowed: bool) -> &mut Self {
        bool_flag!(self.desired_access, allowed, winnt::GENERIC_WRITE);
        self
    }

    /// Sets qos flags which are combined with other flags and attributes in the
    /// call to [`CreateFile`].
    ///
    /// By default `security_qos_flags` is set to [`SECURITY_IDENTIFICATION`],
    /// calling this function would override that value completely with the
    /// argument specified.
    ///
    /// When `security_qos_flags` is not set, a malicious program can gain the
    /// elevated privileges of a privileged Rust process when it allows opening
    /// user-specified paths, by tricking it into opening a named pipe. So
    /// arguably `security_qos_flags` should also be set when opening arbitrary
    /// paths. However the bits can then conflict with other flags, specifically
    /// `FILE_FLAG_OPEN_NO_RECALL`.
    ///
    /// For information about possible values, see [Impersonation Levels] on the
    /// Windows Dev Center site. The `SECURITY_SQOS_PRESENT` flag is set
    /// automatically when using this method.
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    /// [`SECURITY_IDENTIFICATION`]: crate::winapi::um::winbase::SECURITY_IDENTIFICATION
    /// [Impersonation Levels]: https://docs.microsoft.com/en-us/windows/win32/api/winnt/ne-winnt-security_impersonation_level
    pub fn security_qos_flags(&mut self, flags: u32) -> &mut Self {
        // See: https://github.com/rust-lang/rust/pull/58216
        self.security_qos_flags = flags | winbase::SECURITY_SQOS_PRESENT;
        self
    }

    /// Open the named pipe identified by `addr`.
    ///
    /// This constructs the handle using [`CreateFile`].
    ///
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    ///
    /// # Errors
    ///
    /// This errors if called outside of a [Tokio Runtime] which doesn't have
    /// [I/O enabled] or if any OS-specific I/O errors occur.
    ///
    /// There are a few errors you need to take into account when creating a
    /// named pipe on the client side:
    ///
    /// * [`std::io::ErrorKind::NotFound`] - This indicates that the named pipe
    ///   does not exist. Presumably the server is not up.
    /// * [`ERROR_PIPE_BUSY`] - This error is raised when the named pipe exists,
    ///   but the server is not currently waiting for a connection. Please see the
    ///   examples for how to check for this error.
    ///
    /// [`ERROR_PIPE_BUSY`]: crate::winapi::shared::winerror::ERROR_PIPE_BUSY
    /// [I/O enabled]: crate::runtime::Builder::enable_io
    /// [Tokio Runtime]: crate::runtime::Runtime
    /// [`winapi`]: crate::winapi
    ///
    /// A connect loop that waits until a socket becomes available looks like
    /// this:
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use tokio::net::windows::NamedPipeClientOptions;
    /// use tokio::time;
    /// use winapi::shared::winerror;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\mynamedpipe";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let client = loop {
    ///     match NamedPipeClientOptions::new().create(PIPE_NAME) {
    ///         Ok(client) => break client,
    ///         Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
    ///         Err(e) => return Err(e),
    ///     }
    ///
    ///     time::sleep(Duration::from_millis(50)).await;
    /// };
    ///
    /// // use the connected client.
    /// # Ok(()) }
    /// ```
    pub fn create(&self, addr: impl AsRef<OsStr>) -> io::Result<NamedPipe> {
        // Safety: We're calling create_with_security_attributes_raw w/ a null
        // pointer which disables it.
        unsafe { self.create_with_security_attributes_raw(addr, ptr::null_mut()) }
    }

    /// Open the named pipe identified by `addr`.
    ///
    /// This is the same as [`create`] except that it supports providing the raw
    /// pointer to a structure of [`SECURITY_ATTRIBUTES`] which will be passed
    /// as the `lpSecurityAttributes` argument to [`CreateFile`].
    ///
    /// # Safety
    ///
    /// The `attrs` argument must either be null or point at a valid instance of
    /// the [`SECURITY_ATTRIBUTES`] structure. If the argument is null, the
    /// behavior is identical to calling the [`create`] method.
    ///
    /// [`create`]: NamedPipeClientOptions::create
    /// [`CreateFile`]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
    /// [`SECURITY_ATTRIBUTES`]: crate::winapi::um::minwinbase::SECURITY_ATTRIBUTES
    pub unsafe fn create_with_security_attributes_raw(
        &self,
        addr: impl AsRef<OsStr>,
        attrs: *mut c_void,
    ) -> io::Result<NamedPipe> {
        let addr = encode_addr(addr);

        // NB: We could use a platform specialized `OpenOptions` here, but since
        // we have access to winapi it ultimately doesn't hurt to use
        // `CreateFile` explicitly since it allows the use of our already
        // well-structured wide `addr` to pass into CreateFileW.
        let h = fileapi::CreateFileW(
            addr.as_ptr(),
            self.desired_access,
            0,
            attrs as *mut _,
            fileapi::OPEN_EXISTING,
            self.get_flags(),
            ptr::null_mut(),
        );

        if h == handleapi::INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let io = mio_windows::NamedPipe::from_raw_handle(h);
        let io = PollEvented::new(io)?;

        Ok(NamedPipe { io })
    }

    fn get_flags(&self) -> u32 {
        self.security_qos_flags | winbase::FILE_FLAG_OVERLAPPED
    }
}

/// The pipe mode of a [`NamedPipe`].
///
/// Set through [`NamedPipeOptions::pipe_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PipeMode {
    /// Data is written to the pipe as a stream of bytes. The pipe does not
    /// distinguish bytes written during different write operations.
    ///
    /// Corresponds to [`PIPE_TYPE_BYTE`][crate::winapi::um::winbase::PIPE_TYPE_BYTE].
    Byte,
    /// Data is written to the pipe as a stream of messages. The pipe treats the
    /// bytes written during each write operation as a message unit. Any reading
    /// function on [`NamedPipe`] returns [`ERROR_MORE_DATA`] when a message is not
    /// read completely.
    ///
    /// Corresponds to [`PIPE_TYPE_MESSAGE`][crate::winapi::um::winbase::PIPE_TYPE_MESSAGE].
    ///
    /// [`ERROR_MORE_DATA`]: crate::winapi::shared::winerror::ERROR_MORE_DATA
    Message,
}

/// Indicates the end of a [`NamedPipe`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PipeEnd {
    /// The [`NamedPipe`] refers to the client end of a named pipe instance.
    ///
    /// Corresponds to [`PIPE_CLIENT_END`][crate::winapi::um::winbase::PIPE_CLIENT_END].
    Client,
    /// The [`NamedPipe`] refers to the server end of a named pipe instance.
    ///
    /// Corresponds to [`PIPE_SERVER_END`][crate::winapi::um::winbase::PIPE_SERVER_END].
    Server,
}

/// Information about a named pipe.
///
/// Constructed through [`NamedPipe::info`].
#[derive(Debug)]
#[non_exhaustive]
pub struct PipeInfo {
    /// Indicates the mode of a named pipe.
    pub mode: PipeMode,
    /// Indicates the end of a named pipe.
    pub end: PipeEnd,
    /// The maximum number of instances that can be created for this pipe.
    pub max_instances: u32,
    /// The number of bytes to reserve for the output buffer.
    pub out_buffer_size: u32,
    /// The number of bytes to reserve for the input buffer.
    pub in_buffer_size: u32,
}

/// Information about a pipe gained by peeking it.
///
/// See [`NamedPipe::peek`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct PipePeekInfo {
    /// Indicates the total number of bytes available on the pipe.
    pub total_bytes_available: usize,
    /// Indicates the number of bytes left in the current message.
    ///
    /// If the pipe mode is not [`PipeMode::Message`], then this is
    /// zero.
    pub bytes_left_this_message: usize,
}

/// Encode an address so that it is a null-terminated wide string.
fn encode_addr(addr: impl AsRef<OsStr>) -> Box<[u16]> {
    let len = addr.as_ref().encode_wide().count();
    let mut vec = Vec::with_capacity(len + 1);
    vec.extend(addr.as_ref().encode_wide());
    vec.push(0);
    vec.into_boxed_slice()
}
