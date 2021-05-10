use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::mem;
use std::pin::Pin;
use std::ptr;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::io::{AsyncRead, AsyncWrite, Interest, PollEvented, ReadBuf};
use crate::net::asyncify;
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

    // Interned constant to wait forever. Not available in winapi.
    pub(super) const NMPWAIT_WAIT_FOREVER: DWORD = 0xffffffff;
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
/// Constructed using [NamedPipeClientBuilder::create] for clients, or
/// [NamedPipeBuilder::create] for servers. See their corresponding
/// documentation for examples.
///
/// Connecting a client involves a few steps. First we must try to [create], the
/// error typically indicates one of two things:
///
/// * [std::io::ErrorKind::NotFound] - There is no server available.
/// * [ERROR_PIPE_BUSY] - There is a server available, but it is busy. Use
/// [wait] until it becomes available.
///
/// So a typical client connect loop will look like the this:
///
/// ```no_run
/// use std::time::Duration;
/// use tokio::net::windows::NamedPipeClientBuilder;
/// use winapi::shared::winerror;
///
/// const PIPE_NAME: &str = r"\\.\pipe\named-pipe-idiomatic-client";
///
/// # #[tokio::main] async fn main() -> std::io::Result<()> {
/// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
///
/// let client = loop {
///     match client_builder.create() {
///         Ok(client) => break client,
///         Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
///         Err(e) => return Err(e),
///     }
///
///     client_builder.wait(Some(Duration::from_secs(5))).await?;
/// };
///
/// /* use the connected client */
/// # Ok(()) }
/// ```
///
/// A client will error with [std::io::ErrorKind::NotFound] for most creation
/// oriented operations like [create] or [wait] unless at least once server
/// instance is up and running at all time. This means that the typical listen
/// loop for a server is a bit involved, because we have to ensure that we never
/// drop a server accidentally while a client might want to connect.
///
/// ```no_run
/// use std::io;
/// use std::sync::Arc;
/// use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};
/// use tokio::sync::Notify;
///
/// const PIPE_NAME: &str = r"\\.\pipe\named-pipe-idiomatic-server";
///
/// # #[tokio::main] async fn main() -> std::io::Result<()> {
/// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
/// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
///
/// // The first server needs to be constructed early so that clients can
/// // be correctly connected. Otherwise calling .wait will cause the client to
/// // error.
/// //
/// // Here we also make use of `first_pipe_instance`, which will ensure that
/// // there are no other servers up and running already.
/// let mut server = server_builder.clone().first_pipe_instance(true).create()?;
///
/// let shutdown = Arc::new(Notify::new());
/// let shutdown2 = shutdown.clone();
///
/// // Spawn the server loop.
/// let server = tokio::spawn(async move {
///     loop {
///         // Wait for a client to connect.
///         let connected = tokio::select! {
///             connected = server.connect() => connected,
///             _ = shutdown2.notified() => break,
///         };
///
///         // Construct the next server to be connected before sending the one
///         // we already have of onto a task. This ensures that the server
///         // isn't closed (after it's done in the task) before a new one is
///         // available. Otherwise the client might error with
///         // `io::ErrorKind::NotFound`.
///         server = server_builder.create()?;
///
///         let client = tokio::spawn(async move {
///             /* use the connected client */
/// #           Ok::<_, std::io::Error>(())
///         });
///     }
///
///     Ok::<_, io::Error>(())
/// });
/// # shutdown.notify_one();
/// # let _ = server.await??;
/// # Ok(()) }
/// ```
///
/// [create]: NamedPipeClientBuilder::create
/// [ERROR_PIPE_BUSY]: crate::winapi::shared::winerror::ERROR_PIPE_BUSY
/// [wait]: NamedPipeClientBuilder::wait
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
    /// [I/O enabled]: crate::runtime::RuntimeBuilder::enable_io
    unsafe fn try_from_raw_handle(handle: RawHandle) -> io::Result<Self> {
        let named_pipe = mio_windows::NamedPipe::from_raw_handle(handle);

        Ok(NamedPipe {
            io: PollEvented::new(named_pipe)?,
        })
    }

    /// Enables a named pipe server process to wait for a client process to
    /// connect to an instance of a named pipe. A client process connects by
    /// creating a named pipe with the same name.
    ///
    /// This corresponds to the [ConnectNamedPipe] system call.
    ///
    /// [ConnectNamedPipe]: https://docs.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-connectnamedpipe
    ///
    /// ```no_run
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeBuilder::new(r"\\.\pipe\mynamedpipe");
    /// let pipe = builder.create()?;
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
    /// use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};
    /// use winapi::shared::winerror;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-disconnect";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let server = server_builder.create()?;
    ///
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    /// let mut client = client_builder.create()?;
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
    /// use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder, PipeMode, PipeEnd};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-info";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME)
    ///     .pipe_mode(PipeMode::Message)
    ///     .max_instances(5);
    ///
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    ///
    /// let server = server_builder.create()?;
    /// let client = client_builder.create()?;
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

/// Raw handle conversion for [NamedPipe].
///
/// # Panics
///
/// This panics if called outside of a [Tokio Runtime] which doesn't have [I/O
/// enabled].
///
/// [Tokio Runtime]: crate::runtime::Runtime
/// [I/O enabled]: crate::runtime::Builder::enable_io
impl FromRawHandle for NamedPipe {
    unsafe fn from_raw_handle(handle: RawHandle) -> Self {
        Self::try_from_raw_handle(handle).unwrap()
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
/// See [NamedPipeBuilder::create].
#[derive(Clone)]
pub struct NamedPipeBuilder {
    name: Arc<[u16]>,
    open_mode: DWORD,
    pipe_mode: DWORD,
    max_instances: DWORD,
    out_buffer_size: DWORD,
    in_buffer_size: DWORD,
    default_timeout: DWORD,
}

impl NamedPipeBuilder {
    /// Creates a new named pipe builder with the default settings.
    ///
    /// ```
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-new";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let server = server_builder.create()?;
    /// # Ok(()) }
    /// ```
    pub fn new(addr: impl AsRef<OsStr>) -> NamedPipeBuilder {
        NamedPipeBuilder {
            name: addr
                .as_ref()
                .encode_wide()
                .chain(Some(0))
                .collect::<Arc<[u16]>>(),
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
    /// The default pipe mode is [PipeMode::Byte]. See [PipeMode] for
    /// documentation of what each mode means.
    ///
    /// This corresponding to specifying [dwPipeMode].
    ///
    /// [dwPipeMode]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    pub fn pipe_mode(mut self, pipe_mode: PipeMode) -> Self {
        self.pipe_mode = match pipe_mode {
            PipeMode::Byte => winbase::PIPE_TYPE_BYTE,
            PipeMode::Message => winbase::PIPE_TYPE_MESSAGE,
        };

        self
    }

    /// The flow of data in the pipe goes from client to server only.
    ///
    /// This corresponds to setting [PIPE_ACCESS_INBOUND].
    ///
    /// [PIPE_ACCESS_INBOUND]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_access_inbound
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    /// use tokio::net::windows::{NamedPipeClientBuilder, NamedPipeBuilder};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-access-inbound";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    ///
    /// // Server side prevents connecting by denying inbound access, client errors
    /// // when attempting to create the connection.
    /// {
    ///     let server_builder = server_builder.clone().access_inbound(false);
    ///     let _server = server_builder.create()?;
    ///
    ///     let e = client_builder.create().unwrap_err();
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    ///
    ///     // Disabling writing allows a client to connect, but leads to runtime
    ///     // error if a write is attempted.
    ///     let mut client = client_builder.clone().write(false).create()?;
    ///
    ///     let e = client.write(b"ping").await.unwrap_err();
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    /// }
    ///
    /// // A functional, unidirectional server-to-client only communication.
    /// {
    ///     let mut server = server_builder.clone().access_inbound(false).create()?;
    ///     let mut client = client_builder.clone().write(false).create()?;
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
    pub fn access_inbound(mut self, allowed: bool) -> Self {
        bool_flag!(self.open_mode, allowed, winbase::PIPE_ACCESS_INBOUND);
        self
    }

    /// The flow of data in the pipe goes from server to client only.
    ///
    /// This corresponds to setting [PIPE_ACCESS_OUTBOUND].
    ///
    /// [PIPE_ACCESS_OUTBOUND]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_access_outbound
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    /// use tokio::net::windows::{NamedPipeClientBuilder, NamedPipeBuilder};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-access-outbound";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    ///
    /// // Server side prevents connecting by denying outbound access, client errors
    /// // when attempting to create the connection.
    /// {
    ///     let server_builder = server_builder.clone().access_outbound(false);
    ///     let _server = server_builder.create()?;
    ///
    ///     let e = client_builder.create().unwrap_err();
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    ///
    ///     // Disabling reading allows a client to connect, but leads to runtime
    ///     // error if a read is attempted.
    ///     let mut client = client_builder.clone().read(false).create()?;
    ///
    ///     let mut buf = [0u8; 4];
    ///     let e = client.read(&mut buf).await.unwrap_err();
    ///     assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    /// }
    ///
    /// // A functional, unidirectional client-to-server only communication.
    /// {
    ///     let mut server = server_builder.clone().access_outbound(false).create()?;
    ///     let mut client = client_builder.clone().read(false).create()?;
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
    pub fn access_outbound(mut self, allowed: bool) -> Self {
        bool_flag!(self.open_mode, allowed, winbase::PIPE_ACCESS_OUTBOUND);
        self
    }

    /// If you attempt to create multiple instances of a pipe with this flag,
    /// creation of the first instance succeeds, but creation of the next
    /// instance fails with [ERROR_ACCESS_DENIED].
    ///
    /// This corresponds to setting [FILE_FLAG_FIRST_PIPE_INSTANCE].
    ///
    /// [ERROR_ACCESS_DENIED]: crate::winapi::shared::winerror::ERROR_ACCESS_DENIED
    /// [FILE_FLAG_FIRST_PIPE_INSTANCE]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_first_pipe_instance
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-first-instance";
    ///
    /// # #[tokio::main] async fn main() -> io::Result<()> {
    /// let builder = NamedPipeBuilder::new(PIPE_NAME).first_pipe_instance(true);
    ///
    /// let server = builder.create()?;
    /// let e = builder.create().unwrap_err();
    /// assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
    /// drop(server);
    ///
    /// // OK: since, we've closed the other instance.
    /// let _server2 = builder.create()?;
    /// # Ok(()) }
    /// ```
    pub fn first_pipe_instance(mut self, first: bool) -> Self {
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
    /// This corresponds to setting [PIPE_REJECT_REMOTE_CLIENTS].
    ///
    /// [PIPE_REJECT_REMOTE_CLIENTS]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_reject_remote_clients
    pub fn reject_remote_clients(mut self, reject: bool) -> Self {
        bool_flag!(self.pipe_mode, reject, winbase::PIPE_REJECT_REMOTE_CLIENTS);
        self
    }

    /// The maximum number of instances that can be created for this pipe. The
    /// first instance of the pipe can specify this value; the same number must
    /// be specified for other instances of the pipe. Acceptable values are in
    /// the range 1 through 254. The default value is unlimited.
    ///
    /// This corresponds to specifying [nMaxInstances].
    ///
    /// [nMaxInstances]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    /// [PIPE_UNLIMITED_INSTANCES]: crate::winapi::um::winbase::PIPE_UNLIMITED_INSTANCES
    ///
    /// # Panics
    ///
    /// This function will panic if more than 254 instances are specified. If
    /// you do not wish to set an instance limit, leave it unspecified.
    ///
    /// ```should_panic
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeBuilder::new(r"\\.\pipe\mynamedpipe").max_instances(255);
    /// # Ok(()) }
    /// ```
    pub fn max_instances(mut self, instances: usize) -> Self {
        assert!(instances < 255, "cannot specify more than 254 instances");
        self.max_instances = instances as DWORD;
        self
    }

    /// The number of bytes to reserve for the output buffer.
    ///
    /// This corresponds to specifying [nOutBufferSize].
    ///
    /// [nOutBufferSize]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    pub fn out_buffer_size(mut self, buffer: u32) -> Self {
        self.out_buffer_size = buffer as DWORD;
        self
    }

    /// The number of bytes to reserve for the input buffer.
    ///
    /// This corresponds to specifying [nInBufferSize].
    ///
    /// [nInBufferSize]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    pub fn in_buffer_size(mut self, buffer: u32) -> Self {
        self.in_buffer_size = buffer as DWORD;
        self
    }

    /// Create the named pipe identified by the name provided in [new] for use
    /// by a server.
    ///
    /// This function will call the [CreateNamedPipe] function and return the
    /// result.
    ///
    /// [new]: NamedPipeBuilder::new
    /// [CreateNamedPipe]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    ///
    /// ```
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-create";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let server = server_builder.create()?;
    /// # Ok(()) }
    /// ```
    pub fn create(&self) -> io::Result<NamedPipe> {
        // Safety: We're calling create_with_security_attributes w/ a null
        // pointer which disables it.
        unsafe { self.create_with_security_attributes(ptr::null_mut()) }
    }

    /// Create the named pipe identified by the name provided in [new] for use
    /// by a server.
    ///
    /// This is the same as [create][NamedPipeBuilder::create] except that it
    /// supports providing security attributes.
    ///
    /// [new]: NamedPipeBuilder::new
    ///
    /// # Safety
    ///
    /// The caller must ensure that `attrs` points to an initialized instance of
    /// a [SECURITY_ATTRIBUTES] structure.
    ///
    /// [SECURITY_ATTRIBUTES]: [crate::winapi::um::minwinbase::SECURITY_ATTRIBUTES]
    pub unsafe fn create_with_security_attributes(&self, attrs: *mut ()) -> io::Result<NamedPipe> {
        let h = namedpipeapi::CreateNamedPipeW(
            self.name.as_ptr(),
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

impl fmt::Debug for NamedPipeBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = String::from_utf16_lossy(&self.name[..self.name.len() - 1]);
        f.debug_struct("NamedPipeBuilder")
            .field("name", &name)
            .finish()
    }
}

/// A builder suitable for building and interacting with named pipes from the
/// client side.
///
/// See [NamedPipeClientBuilder::create].
#[derive(Clone)]
pub struct NamedPipeClientBuilder {
    name: Arc<[u16]>,
    desired_access: DWORD,
}

impl NamedPipeClientBuilder {
    /// Creates a new named pipe builder with the default settings.
    ///
    /// ```
    /// use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-client-new";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// // Server must be created in order for the client creation to succeed.
    /// let server = NamedPipeBuilder::new(PIPE_NAME).create()?;
    ///
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    /// let client = client_builder.create()?;
    /// # Ok(()) }
    /// ```
    pub fn new(addr: impl AsRef<OsStr>) -> Self {
        Self {
            name: addr
                .as_ref()
                .encode_wide()
                .chain(Some(0))
                .collect::<Arc<[u16]>>(),
            desired_access: winnt::GENERIC_READ | winnt::GENERIC_WRITE,
        }
    }

    /// Waits until either a time-out interval elapses or an instance of the
    /// specified named pipe is available for connection (that is, the pipe's
    /// server process has a pending [connect] operation on the pipe).
    ///
    /// This corresponds to the [`WaitNamedPipeW`] system call.
    ///
    /// [`WaitNamedPipeW`]:
    /// https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-waitnamedpipea
    /// [connect]: NamedPipe::connect
    ///
    /// # Errors
    ///
    /// If a server hasn't already created the named pipe, this will return an
    /// error with the kind [std::io::ErrorKind::NotFound].
    ///
    /// ```
    /// use std::io;
    /// use std::time::Duration;
    /// use tokio::net::windows::NamedPipeClientBuilder;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-client-wait-error1";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    ///
    /// let e = client_builder.wait(Some(Duration::from_secs(1))).await.unwrap_err();
    ///
    /// // Errors because no server exists.
    /// assert_eq!(e.kind(), io::ErrorKind::NotFound);
    /// # Ok(()) }
    /// ```
    ///
    /// Waiting while a server is being closed will first cause it to block, but
    /// then error with [std::io::ErrorKind::NotFound].
    ///
    /// ```
    /// use std::io;
    /// use std::time::Duration;
    /// use tokio::net::windows::{NamedPipeClientBuilder, NamedPipeBuilder};
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-client-wait-error2";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    ///
    /// let server = server_builder.create()?;
    ///
    /// // Construct a client that occupies the server so that the next one is
    /// // forced to wait.
    /// let _client = client_builder.create()?;
    ///
    /// tokio::spawn(async move {
    ///     // Drop the server after 100ms, causing the waiting client to err.
    ///     tokio::time::sleep(Duration::from_millis(100)).await;
    ///     drop(server);
    /// });
    ///
    /// let e = client_builder.wait(Some(Duration::from_secs(1))).await.unwrap_err();
    ///
    /// assert_eq!(e.kind(), io::ErrorKind::NotFound);
    /// # Ok(()) }
    /// ```
    ///
    /// # Examples
    ///
    /// ```
    /// use std::io;
    /// use std::time::Duration;
    /// use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    /// use tokio::net::windows::{NamedPipeBuilder, NamedPipeClientBuilder};
    /// use winapi::shared::winerror;
    ///
    /// const PIPE_NAME: &str = r"\\.\pipe\tokio-named-pipe-client-wait";
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let server_builder = NamedPipeBuilder::new(PIPE_NAME);
    /// let client_builder = NamedPipeClientBuilder::new(PIPE_NAME);
    ///
    /// // The first server needs to be constructed early so that clients can
    /// // be correctly connected. Otherwise calling .wait will cause the client
    /// // to error because the file descriptor doesn't exist.
    /// //
    /// // Here we also make use of `first_pipe_instance`, which will ensure
    /// // that there are no other servers up and running already.
    /// let mut server = server_builder.clone().first_pipe_instance(true).create()?;
    ///
    /// let client = tokio::spawn(async move {
    ///     // Wait forever until a socket is available to be connected to.
    ///     let mut client = loop {
    ///         match client_builder.create() {
    ///             Ok(client) => break client,
    ///             Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
    ///             Err(e) => return Err(e),
    ///         }
    ///
    ///         client_builder.wait(Some(Duration::from_secs(5))).await?;
    ///     };
    ///
    ///     let mut buf = [0u8; 4];
    ///     client.read_exact(&mut buf[..]).await?;
    ///     Ok::<_, io::Error>(buf)
    /// });
    ///
    /// let server = tokio::spawn(async move {
    ///     tokio::time::sleep(Duration::from_millis(200)).await;
    ///
    ///     // Calling `connect` is necessary for the waiting client to wake up,
    ///     // even if the server is created after the client.
    ///     server.connect().await?;
    ///
    ///     server.write_all(b"ping").await?;
    ///     Ok::<_, io::Error>(())
    /// });
    ///
    /// let (client, server) = tokio::try_join!(client, server)?;
    /// let payload = client?;
    /// assert_eq!(&payload[..], b"ping");
    /// let _ = server?;
    /// # Ok(()) }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if the specified duration is larger than `0xffffffff`
    /// milliseconds, which is roughly equal to 1193 hours.
    ///
    /// ```should_panic
    /// use std::time::Duration;
    /// use tokio::net::windows::NamedPipeClientBuilder;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeClientBuilder::new(r"\\.\pipe\mynamedpipe");
    ///
    /// builder.wait(Some(Duration::from_millis(0xffffffff))).await?;
    /// # Ok(()) }
    /// ```
    pub async fn wait(&self, timeout: Option<Duration>) -> io::Result<()> {
        let timeout = match timeout {
            Some(timeout) => {
                let timeout = timeout.as_millis();
                assert! {
                    timeout < NMPWAIT_WAIT_FOREVER as u128,
                    "timeout out of bounds, can wait at most {}ms, but got {}ms", NMPWAIT_WAIT_FOREVER - 1,
                    timeout
                };
                timeout as DWORD
            }
            None => NMPWAIT_WAIT_FOREVER,
        };

        let name = self.name.clone();

        // TODO: Is this the right thread pool to use? `WaitNamedPipeW` could
        // potentially block for a fairly long time all though it's only
        // expected to be used when connecting something which should be fairly
        // constrained.
        //
        // But doing something silly like spawning hundreds of clients trying to
        // connect to a named pipe server without a timeout could easily end up
        // starving the thread pool.
        let task = asyncify(move || {
            // Safety: There's nothing unsafe about this.
            let result = unsafe { namedpipeapi::WaitNamedPipeW(name.as_ptr(), timeout) };

            if result == FALSE {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        });

        task.await
    }

    /// If the client supports reading data. This is enabled by default.
    ///
    /// This corresponds to setting [GENERIC_READ] in the call to [CreateFile].
    ///
    /// [GENERIC_READ]: https://docs.microsoft.com/en-us/windows/win32/secauthz/generic-access-rights
    /// [CreateFile]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
    pub fn read(mut self, allowed: bool) -> Self {
        bool_flag!(self.desired_access, allowed, winnt::GENERIC_READ);
        self
    }

    /// If the created pipe supports writing data. This is enabled by default.
    ///
    /// This corresponds to setting [GENERIC_WRITE] in the call to [CreateFile].
    ///
    /// [GENERIC_WRITE]: https://docs.microsoft.com/en-us/windows/win32/secauthz/generic-access-rights
    /// [CreateFile]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew
    pub fn write(mut self, allowed: bool) -> Self {
        bool_flag!(self.desired_access, allowed, winnt::GENERIC_WRITE);
        self
    }

    /// Open the named pipe identified by the name provided in [new].
    ///
    /// This constructs the handle using [CreateFile].
    ///
    /// [new]: NamedPipeClientBuilder::new
    /// [CreateFile]: https://docs.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilea
    ///
    /// # Errors
    ///
    /// There are a few errors you should be aware of that you need to take into
    /// account when creating a named pipe on the client side.
    ///
    /// * [std::io::ErrorKind::NotFound] - This indicates that the named pipe
    ///   does not exist. Presumably the server is not up.
    /// * [ERROR_PIPE_BUSY] - which needs to be tested for through a constant in
    ///   [winapi]. This error is raised when the named pipe has been created,
    ///   but the server is not currently waiting for a connection.
    ///
    /// [ERROR_PIPE_BUSY]: crate::winapi::shared::winerror::ERROR_PIPE_BUSY
    /// [winapi]: crate::winapi
    ///
    /// The generic connect loop looks like this.
    ///
    /// ```no_run
    /// use std::io;
    /// use std::time::Duration;
    /// use tokio::net::windows::NamedPipeClientBuilder;
    /// use winapi::shared::winerror;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeClientBuilder::new(r"\\.\pipe\mynamedpipe");
    ///
    /// let client = loop {
    ///     match builder.create() {
    ///         Ok(client) => break client,
    ///         Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
    ///         Err(e) => return Err(e),
    ///     }
    ///
    ///     if builder.wait(Some(Duration::from_secs(5))).await.is_err() {
    ///         return Err(io::Error::new(io::ErrorKind::Other, "server timed out"));
    ///     }
    /// };
    ///
    /// // use the connected client.
    /// # Ok(()) }
    /// ```
    pub fn create(&self) -> io::Result<NamedPipe> {
        // Safety: We're calling create_with_security_attributes w/ a null
        // pointer which disables it.
        unsafe { self.create_with_security_attributes(ptr::null_mut()) }
    }

    /// Open the named pipe identified by the name provided in [new].
    ///
    /// This is the same as [create][NamedPipeClientBuilder::create] except that
    /// it supports providing security attributes.
    ///
    /// [new]: NamedPipeClientBuilder::new
    ///
    /// # Safety
    ///
    /// The caller must ensure that `attrs` points to an initialized instance
    /// of a [SECURITY_ATTRIBUTES] structure.
    ///
    /// [SECURITY_ATTRIBUTES]: [crate::winapi::um::minwinbase::SECURITY_ATTRIBUTES]
    pub unsafe fn create_with_security_attributes(&self, attrs: *mut ()) -> io::Result<NamedPipe> {
        // NB: We could use a platform specialized `OpenOptions` here, but since
        // we have access to winapi it ultimately doesn't hurt to use
        // `CreateFile` explicitly since it allows the use of our already
        // well-structured wide `name` to pass into CreateFileW.
        let h = fileapi::CreateFileW(
            self.name.as_ptr(),
            self.desired_access,
            0,
            attrs as *mut _,
            fileapi::OPEN_EXISTING,
            winbase::FILE_FLAG_OVERLAPPED | winbase::SECURITY_IDENTIFICATION,
            ptr::null_mut(),
        );

        if h == handleapi::INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let io = mio_windows::NamedPipe::from_raw_handle(h);
        let io = PollEvented::new(io)?;

        Ok(NamedPipe { io })
    }
}

impl fmt::Debug for NamedPipeClientBuilder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = String::from_utf16_lossy(&self.name[..self.name.len() - 1]);
        f.debug_struct("NamedPipeClientBuilder")
            .field("name", &name)
            .finish()
    }
}

/// The pipe mode of a [NamedPipe].
///
/// Set through [NamedPipeBuilder::pipe_mode].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PipeMode {
    /// Data is written to the pipe as a stream of bytes. The pipe does not
    /// distinguish bytes written during different write operations.
    ///
    /// Corresponds to [PIPE_TYPE_BYTE][crate::winapi::um::winbase::PIPE_TYPE_BYTE].
    Byte,
    /// Data is written to the pipe as a stream of messages. The pipe treats the
    /// bytes written during each write operation as a message unit. Any reading
    /// function on [NamedPipe] returns [ERROR_MORE_DATA] when a message is not
    /// read completely.
    ///
    /// Corresponds to [PIPE_TYPE_MESSAGE][crate::winapi::um::winbase::PIPE_TYPE_MESSAGE].
    ///
    /// [ERROR_MORE_DATA]: crate::winapi::shared::winerror::ERROR_MORE_DATA
    Message,
}

/// Indicates the end of a [NamedPipe].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum PipeEnd {
    /// The [NamedPipe] refers to the client end of a named pipe instance.
    ///
    /// Corresponds to [PIPE_CLIENT_END][crate::winapi::um::winbase::PIPE_CLIENT_END].
    Client,
    /// The [NamedPipe] refers to the server end of a named pipe instance.
    ///
    /// Corresponds to [PIPE_SERVER_END][crate::winapi::um::winbase::PIPE_SERVER_END].
    Server,
}

/// Information about a named pipe.
///
/// Constructed through [NamedPipe::info].
#[derive(Debug)]
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
