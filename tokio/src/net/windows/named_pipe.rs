use std::ffi::OsStr;
use std::fmt;
use std::io;
use std::os::windows::ffi::OsStrExt as _;
use std::os::windows::io::RawHandle;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::pin::Pin;
use std::ptr;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;
use winapi::shared::minwindef::DWORD;
use winapi::um::fileapi;
use winapi::um::handleapi;
use winapi::um::minwinbase;
use winapi::um::namedpipeapi;
use winapi::um::winbase;
use winapi::um::winnt;

use crate::fs::asyncify;
use crate::io::{AsyncRead, AsyncWrite, Interest, PollEvented, ReadBuf};

// Interned constant to wait forever. Not available in winapi.
const NMPWAIT_WAIT_FOREVER: DWORD = 0xffffffff;

/// A [Windows named pipe].
///
/// [Windows named pipe]: https://docs.microsoft.com/en-us/windows/win32/ipc/named-pipes
///
/// Constructed using [NamedPipeClientBuilder::create] for clients, or
/// [NamedPipeBuilder::create] for servers. See their corresponding
/// documentation for examples.
#[derive(Debug)]
pub struct NamedPipe {
    io: PollEvented<mio::windows::NamedPipe>,
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
        let named_pipe = mio::windows::NamedPipe::from_raw_handle(handle);

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
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeBuilder::new("\\\\.\\pipe\\mynamedpipe");
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
    /// ```no_run
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeBuilder::new("\\\\.\\pipe\\mynamedpipe");
    /// let pipe = builder.create()?;
    ///
    /// // Wait for a client to connect.
    /// pipe.connect().await?;
    ///
    /// // Use the pipe...
    ///
    /// // Forcibly disconnect the client (optional).
    /// pipe.disconnect()?;
    /// # Ok(()) }
    /// ```
    pub fn disconnect(&self) -> io::Result<()> {
        self.io.disconnect()
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
/// [I/O enabled]: crate::runtime::RuntimeBuilder::enable_io
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
    /// ```no_run
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeBuilder::new("\\\\.\\pipe\\mynamedpipe");
    /// let pipe = builder.create()?;
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
            pipe_mode: winbase::PIPE_TYPE_BYTE,
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
    /// [nInBufferSize]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
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
    pub fn access_inbound(mut self, allowed: bool) -> Self {
        bool_flag!(self.open_mode, allowed, winbase::PIPE_ACCESS_INBOUND);
        self
    }

    /// The flow of data in the pipe goes from server to client only.
    ///
    /// This corresponds to setting [PIPE_ACCESS_OUTBOUND].
    ///
    /// [PIPE_ACCESS_OUTBOUND]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_access_outbound
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
    /// [ERROR_ACCESS_DENIED]: winapi::shared::winerror::ERROR_ACCESS_DENIED
    /// [FILE_FLAG_FIRST_PIPE_INSTANCE]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea#pipe_first_pipe_instance
    pub fn first_pipe_instance(mut self, first: bool) -> Self {
        bool_flag!(
            self.open_mode,
            first,
            winbase::FILE_FLAG_FIRST_PIPE_INSTANCE
        );
        self
    }

    /// Indicates whether this server can accept remote clients or not.
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
    /// the range 1 through 254.
    ///
    /// The default value is unlimited. This corresponds to specifying
    /// [nMaxInstances].
    ///
    /// [nMaxInstances]: https://docs.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea
    /// [PIPE_UNLIMITED_INSTANCES]: winapi::um::winbase::PIPE_UNLIMITED_INSTANCES
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
    /// let builder = NamedPipeBuilder::new("\\\\.\\pipe\\mynamedpipe").max_instances(255);
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
    /// ```no_run
    /// use tokio::net::windows::NamedPipeBuilder;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeBuilder::new("\\\\.\\pipe\\mynamedpipe");
    /// let pipe = builder.create()?;
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
    /// [SECURITY_ATTRIBUTES]: [winapi::um::minwinbase::SECURITY_ATTRIBUTES]
    pub unsafe fn create_with_security_attributes(&self, attrs: *mut ()) -> io::Result<NamedPipe> {
        let h = namedpipeapi::CreateNamedPipeW(
            self.name.as_ptr(),
            self.open_mode,
            self.pipe_mode,
            self.max_instances,
            self.out_buffer_size,
            self.in_buffer_size,
            self.default_timeout,
            attrs as *mut minwinbase::SECURITY_ATTRIBUTES,
        );

        if h == handleapi::INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let io = mio::windows::NamedPipe::from_raw_handle(h);
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
}

impl NamedPipeClientBuilder {
    /// Creates a new named pipe builder with the default settings.
    ///
    /// ```no_run
    /// use tokio::net::windows::NamedPipeClientBuilder;
    ///
    /// # fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeClientBuilder::new("\\\\.\\pipe\\mynamedpipe");
    /// let pipe = builder.create()?;
    /// # Ok(()) }
    /// ```
    pub fn new(addr: impl AsRef<OsStr>) -> Self {
        Self {
            name: addr
                .as_ref()
                .encode_wide()
                .chain(Some(0))
                .collect::<Arc<[u16]>>(),
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
    /// ```no_run
    /// use tokio::net::windows::NamedPipeClientBuilder;
    ///
    /// # #[tokio::main] async fn main() -> std::io::Result<()> {
    /// let builder = NamedPipeClientBuilder::new("\\\\.\\pipe\\mynamedpipe");
    ///
    /// // Wait forever until a socket is available to be connected to.
    /// builder.wait(None).await?;
    ///
    /// let pipe = builder.create()?;
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
    /// let builder = NamedPipeClientBuilder::new("\\\\.\\pipe\\mynamedpipe");
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

            if result == 0 {
                return Err(io::Error::last_os_error());
            }

            Ok(())
        });

        task.await
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
    /// * [ERROR_FILE_NOT_FOUND] - Which can be tested for using
    ///   [std::io::ErrorKind::NotFound]. This indicates that the named pipe
    ///   does not exist. Presumably the server is not up.
    /// * [ERROR_PIPE_BUSY] - which needs to be tested for through a constant in
    ///   [winapi]. This error is raised when the named pipe has been created,
    ///   but the server is not currently waiting for a connection.
    ///
    /// [ERROR_FILE_NOT_FOUND]: winapi::shared::winerror::ERROR_FILE_NOT_FOUND
    /// [ERROR_PIPE_BUSY]: winapi::shared::winerror::ERROR_PIPE_BUSY
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
    /// let builder = NamedPipeClientBuilder::new("\\\\.\\pipe\\mynamedpipe");
    ///
    /// let client = loop {
    ///     match builder.create() {
    ///         Ok(client) => break client,
    ///         Err(e) if e.raw_os_error() == Some(winerror::ERROR_PIPE_BUSY as i32) => (),
    ///         Err(e) if e.kind() == io::ErrorKind::NotFound => (),
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
    /// [SECURITY_ATTRIBUTES]: [winapi::um::minwinbase::SECURITY_ATTRIBUTES]
    pub unsafe fn create_with_security_attributes(&self, attrs: *mut ()) -> io::Result<NamedPipe> {
        // NB: We could use a platform specialized `OpenOptions` here, but since
        // we have access to winapi it ultimately doesn't hurt to use
        // `CreateFile` explicitly since it allows the use of our already
        // well-structured wide `name` to pass into CreateFileW.
        let h = fileapi::CreateFileW(
            self.name.as_ptr(),
            winnt::GENERIC_READ | winnt::GENERIC_WRITE,
            0,
            attrs as *mut minwinbase::SECURITY_ATTRIBUTES,
            fileapi::OPEN_EXISTING,
            winbase::FILE_FLAG_OVERLAPPED | winbase::SECURITY_IDENTIFICATION,
            ptr::null_mut(),
        );

        if h == handleapi::INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        let io = mio::windows::NamedPipe::from_raw_handle(h);
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
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum PipeMode {
    /// Data is written to the pipe as a stream of bytes. The pipe does not
    /// distinguish bytes written during different write operations.
    Byte,
    /// Data is written to the pipe as a stream of messages. The pipe treats the
    /// bytes written during each write operation as a message unit. Any reading
    /// function on [NamedPipe] returns [ERROR_MORE_DATA] when a message is not
    /// read completely.
    ///
    /// [ERROR_MORE_DATA]: winapi::shared::winerror::ERROR_MORE_DATA
    Message,
}
