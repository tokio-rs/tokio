//! An implementation of asynchronous process management for Tokio.
//!
//! This module provides a [`Command`] struct that imitates the interface of the
//! [`std::process::Command`] type in the standard library, but provides asynchronous versions of
//! functions that create processes. These functions (`spawn`, `status`, `output` and their
//! variants) return "future aware" types that interoperate with Tokio. The asynchronous process
//! support is provided through signal handling on Unix and system APIs on Windows.
//!
//! # Examples
//!
//! Here's an example program which will spawn `echo hello world` and then wait
//! for it complete.
//!
//! ```no_run
//! use tokio::process::Command;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // The usage is the same as with the standard library's `Command` type, however the value
//!     // returned from `spawn` is a `Result` containing a `Future`.
//!     let child = Command::new("echo").arg("hello").arg("world")
//!                         .spawn();
//!
//!     // Make sure our child succeeded in spawning and process the result
//!     let future = child.expect("failed to spawn");
//!
//!     // Await until the future (and the command) completes
//!     let status = future.await?;
//!     println!("the command exited with: {}", status);
//!     Ok(())
//! }
//! ```
//!
//! Next, let's take a look at an example where we not only spawn `echo hello
//! world` but we also capture its output.
//!
//! ```no_run
//! use tokio::process::Command;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Like above, but use `output` which returns a future instead of
//!     // immediately returning the `Child`.
//!     let output = Command::new("echo").arg("hello").arg("world")
//!                         .output();
//!
//!     let output = output.await?;
//!
//!     assert!(output.status.success());
//!     assert_eq!(output.stdout, b"hello world\n");
//!     Ok(())
//! }
//! ```
//!
//! We can also read input line by line.
//!
//! ```no_run
//! use tokio::io::{BufReader, AsyncBufReadExt};
//! use tokio::process::Command;
//!
//! use std::process::Stdio;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let mut cmd = Command::new("cat");
//!
//!     // Specify that we want the command's standard output piped back to us.
//!     // By default, standard input/output/error will be inherited from the
//!     // current process (for example, this means that standard input will
//!     // come from the keyboard and standard output/error will go directly to
//!     // the terminal if this process is invoked from the command line).
//!     cmd.stdout(Stdio::piped());
//!
//!     let mut child = cmd.spawn()
//!         .expect("failed to spawn command");
//!
//!     let stdout = child.stdout.take()
//!         .expect("child did not have a handle to stdout");
//!
//!     let mut reader = BufReader::new(stdout).lines();
//!
//!     // Ensure the child process is spawned in the runtime so it can
//!     // make progress on its own while we await for any output.
//!     tokio::spawn(async {
//!         let status = child.await
//!             .expect("child process encountered an error");
//!
//!         println!("child status was: {}", status);
//!     });
//!
//!     while let Some(line) = reader.next_line().await? {
//!         println!("Line: {}", line);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! # Caveats
//!
//! Similar to the behavior to the standard library, and unlike the futures
//! paradigm of dropping-implies-cancellation, a spawned process will, by
//! default, continue to execute even after the `Child` handle has been dropped.
//!
//! The `Command::kill_on_drop` method can be used to modify this behavior
//! and kill the child process if the `Child` wrapper is dropped before it
//! has exited.
//!
//! [`Command`]: crate::process::Command

#[path = "unix/mod.rs"]
#[cfg(unix)]
mod imp;

#[path = "windows.rs"]
#[cfg(windows)]
mod imp;

mod kill;

use crate::io::{AsyncRead, AsyncWrite};
use crate::process::kill::Kill;

use std::ffi::OsStr;
use std::future::Future;
use std::io;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::pin::Pin;
use std::process::{Command as StdCommand, ExitStatus, Output, Stdio};
use std::task::Context;
use std::task::Poll;

/// This structure mimics the API of [`std::process::Command`] found in the standard library, but
/// replaces functions that create a process with an asynchronous variant. The main provided
/// asynchronous functions are [spawn](Command::spawn), [status](Command::status), and
/// [output](Command::output).
///
/// `Command` uses asynchronous versions of some `std` types (for example [`Child`]).
#[derive(Debug)]
pub struct Command {
    std: StdCommand,
    kill_on_drop: bool,
}

pub(crate) struct SpawnedChild {
    child: imp::Child,
    stdin: Option<imp::ChildStdin>,
    stdout: Option<imp::ChildStdout>,
    stderr: Option<imp::ChildStderr>,
}

impl Command {
    /// Constructs a new `Command` for launching the program at
    /// path `program`, with the following default configuration:
    ///
    /// * No arguments to the program
    /// * Inherit the current process's environment
    /// * Inherit the current process's working directory
    /// * Inherit stdin/stdout/stderr for `spawn` or `status`, but create pipes for `output`
    ///
    /// Builder methods are provided to change these defaults and
    /// otherwise configure the process.
    ///
    /// If `program` is not an absolute path, the `PATH` will be searched in
    /// an OS-defined way.
    ///
    /// The search path to be used may be controlled by setting the
    /// `PATH` environment variable on the Command,
    /// but this has some implementation limitations on Windows
    /// (see issue rust-lang/rust#37519).
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    /// let command = Command::new("sh");
    /// ```
    pub fn new<S: AsRef<OsStr>>(program: S) -> Command {
        Self::from(StdCommand::new(program))
    }

    /// Adds an argument to pass to the program.
    ///
    /// Only one argument can be passed per use. So instead of:
    ///
    /// ```no_run
    /// tokio::process::Command::new("sh")
    ///   .arg("-C /path/to/repo");
    /// ```
    ///
    /// usage would be:
    ///
    /// ```no_run
    /// tokio::process::Command::new("sh")
    ///   .arg("-C")
    ///   .arg("/path/to/repo");
    /// ```
    ///
    /// To pass multiple arguments see [`args`].
    ///
    /// [`args`]: #method.args
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .arg("-l")
    ///         .arg("-a");
    /// ```
    pub fn arg<S: AsRef<OsStr>>(&mut self, arg: S) -> &mut Command {
        self.std.arg(arg);
        self
    }

    /// Adds multiple arguments to pass to the program.
    ///
    /// To pass a single argument see [`arg`].
    ///
    /// [`arg`]: #method.arg
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .args(&["-l", "-a"]);
    /// ```
    pub fn args<I, S>(&mut self, args: I) -> &mut Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.std.args(args);
        self
    }

    /// Inserts or updates an environment variable mapping.
    ///
    /// Note that environment variable names are case-insensitive (but case-preserving) on Windows,
    /// and case-sensitive on all other platforms.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .env("PATH", "/bin");
    /// ```
    pub fn env<K, V>(&mut self, key: K, val: V) -> &mut Command
    where
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.std.env(key, val);
        self
    }

    /// Adds or updates multiple environment variable mappings.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    /// use std::process::{Stdio};
    /// use std::env;
    /// use std::collections::HashMap;
    ///
    /// let filtered_env : HashMap<String, String> =
    ///     env::vars().filter(|&(ref k, _)|
    ///         k == "TERM" || k == "TZ" || k == "LANG" || k == "PATH"
    ///     ).collect();
    ///
    /// let command = Command::new("printenv")
    ///         .stdin(Stdio::null())
    ///         .stdout(Stdio::inherit())
    ///         .env_clear()
    ///         .envs(&filtered_env);
    /// ```
    pub fn envs<I, K, V>(&mut self, vars: I) -> &mut Command
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
    {
        self.std.envs(vars);
        self
    }

    /// Removes an environment variable mapping.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .env_remove("PATH");
    /// ```
    pub fn env_remove<K: AsRef<OsStr>>(&mut self, key: K) -> &mut Command {
        self.std.env_remove(key);
        self
    }

    /// Clears the entire environment map for the child process.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .env_clear();
    /// ```
    pub fn env_clear(&mut self) -> &mut Command {
        self.std.env_clear();
        self
    }

    /// Sets the working directory for the child process.
    ///
    /// # Platform-specific behavior
    ///
    /// If the program path is relative (e.g., `"./script.sh"`), it's ambiguous
    /// whether it should be interpreted relative to the parent's working
    /// directory or relative to `current_dir`. The behavior in this case is
    /// platform specific and unstable, and it's recommended to use
    /// [`canonicalize`] to get an absolute program path instead.
    ///
    /// [`canonicalize`]: crate::fs::canonicalize()
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .current_dir("/bin");
    /// ```
    pub fn current_dir<P: AsRef<Path>>(&mut self, dir: P) -> &mut Command {
        self.std.current_dir(dir);
        self
    }

    /// Sets configuration for the child process's standard input (stdin) handle.
    ///
    /// Defaults to [`inherit`] when used with `spawn` or `status`, and
    /// defaults to [`piped`] when used with `output`.
    ///
    /// [`inherit`]: std::process::Stdio::inherit
    /// [`piped`]: std::process::Stdio::piped
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use std::process::{Stdio};
    /// use tokio::process::Command;
    ///
    /// let command = Command::new("ls")
    ///         .stdin(Stdio::null());
    /// ```
    pub fn stdin<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command {
        self.std.stdin(cfg);
        self
    }

    /// Sets configuration for the child process's standard output (stdout) handle.
    ///
    /// Defaults to [`inherit`] when used with `spawn` or `status`, and
    /// defaults to [`piped`] when used with `output`.
    ///
    /// [`inherit`]: std::process::Stdio::inherit
    /// [`piped`]: std::process::Stdio::piped
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;;
    /// use std::process::Stdio;
    ///
    /// let command = Command::new("ls")
    ///         .stdout(Stdio::null());
    /// ```
    pub fn stdout<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command {
        self.std.stdout(cfg);
        self
    }

    /// Sets configuration for the child process's standard error (stderr) handle.
    ///
    /// Defaults to [`inherit`] when used with `spawn` or `status`, and
    /// defaults to [`piped`] when used with `output`.
    ///
    /// [`inherit`]: std::process::Stdio::inherit
    /// [`piped`]: std::process::Stdio::piped
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;;
    /// use std::process::{Stdio};
    ///
    /// let command = Command::new("ls")
    ///         .stderr(Stdio::null());
    /// ```
    pub fn stderr<T: Into<Stdio>>(&mut self, cfg: T) -> &mut Command {
        self.std.stderr(cfg);
        self
    }

    /// Controls whether a `kill` operation should be invoked on a spawned child
    /// process when its corresponding `Child` handle is dropped.
    ///
    /// By default, this value is assumed to be `false`, meaning the next spawned
    /// process will not be killed on drop, similar to the behavior of the standard
    /// library.
    pub fn kill_on_drop(&mut self, kill_on_drop: bool) -> &mut Command {
        self.kill_on_drop = kill_on_drop;
        self
    }

    /// Sets the [process creation flags][1] to be passed to `CreateProcess`.
    ///
    /// These will always be ORed with `CREATE_UNICODE_ENVIRONMENT`.
    ///
    /// [1]: https://msdn.microsoft.com/en-us/library/windows/desktop/ms684863(v=vs.85).aspx
    #[cfg(windows)]
    pub fn creation_flags(&mut self, flags: u32) -> &mut Command {
        self.std.creation_flags(flags);
        self
    }

    /// Sets the child process's user ID. This translates to a
    /// `setuid` call in the child process. Failure in the `setuid`
    /// call will cause the spawn to fail.
    #[cfg(unix)]
    pub fn uid(&mut self, id: u32) -> &mut Command {
        self.std.uid(id);
        self
    }

    /// Similar to `uid` but sets the group ID of the child process. This has
    /// the same semantics as the `uid` field.
    #[cfg(unix)]
    pub fn gid(&mut self, id: u32) -> &mut Command {
        self.std.gid(id);
        self
    }

    /// Schedules a closure to be run just before the `exec` function is
    /// invoked.
    ///
    /// The closure is allowed to return an I/O error whose OS error code will
    /// be communicated back to the parent and returned as an error from when
    /// the spawn was requested.
    ///
    /// Multiple closures can be registered and they will be called in order of
    /// their registration. If a closure returns `Err` then no further closures
    /// will be called and the spawn operation will immediately return with a
    /// failure.
    ///
    /// # Safety
    ///
    /// This closure will be run in the context of the child process after a
    /// `fork`. This primarily means that any modifications made to memory on
    /// behalf of this closure will **not** be visible to the parent process.
    /// This is often a very constrained environment where normal operations
    /// like `malloc` or acquiring a mutex are not guaranteed to work (due to
    /// other threads perhaps still running when the `fork` was run).
    ///
    /// This also means that all resources such as file descriptors and
    /// memory-mapped regions got duplicated. It is your responsibility to make
    /// sure that the closure does not violate library invariants by making
    /// invalid use of these duplicates.
    ///
    /// When this closure is run, aspects such as the stdio file descriptors and
    /// working directory have successfully been changed, so output to these
    /// locations may not appear where intended.
    #[cfg(unix)]
    pub unsafe fn pre_exec<F>(&mut self, f: F) -> &mut Command
    where
        F: FnMut() -> io::Result<()> + Send + Sync + 'static,
    {
        self.std.pre_exec(f);
        self
    }

    /// Executes the command as a child process, returning a handle to it.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    ///
    /// This method will spawn the child process synchronously and return a
    /// handle to a future-aware child process. The `Child` returned implements
    /// `Future` itself to acquire the `ExitStatus` of the child, and otherwise
    /// the `Child` has methods to acquire handles to the stdin, stdout, and
    /// stderr streams.
    ///
    /// All I/O this child does will be associated with the current default
    /// event loop.
    ///
    /// # Caveats
    ///
    /// Similar to the behavior to the standard library, and unlike the futures
    /// paradigm of dropping-implies-cancellation, the spawned process will, by
    /// default, continue to execute even after the `Child` handle has been dropped.
    ///
    /// The `Command::kill_on_drop` method can be used to modify this behavior
    /// and kill the child process if the `Child` wrapper is dropped before it
    /// has exited.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// async fn run_ls() -> std::process::ExitStatus {
    ///     Command::new("ls")
    ///         .spawn()
    ///         .expect("ls command failed to start")
    ///         .await
    ///         .expect("ls command failed to run")
    /// }
    /// ```
    pub fn spawn(&mut self) -> io::Result<Child> {
        imp::spawn_child(&mut self.std).map(|spawned_child| Child {
            child: ChildDropGuard {
                inner: spawned_child.child,
                kill_on_drop: self.kill_on_drop,
            },
            stdin: spawned_child.stdin.map(|inner| ChildStdin { inner }),
            stdout: spawned_child.stdout.map(|inner| ChildStdout { inner }),
            stderr: spawned_child.stderr.map(|inner| ChildStderr { inner }),
        })
    }

    /// Executes the command as a child process, waiting for it to finish and
    /// collecting its exit status.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent.
    /// If any input/output handles are set to a pipe then they will be immediately
    /// closed after the child is spawned.
    ///
    /// All I/O this child does will be associated with the current default
    /// event loop.
    ///
    /// If this future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    ///
    /// # Errors
    ///
    /// This future will return an error if the child process cannot be spawned
    /// or if there is an error while awaiting its status.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// async fn run_ls() -> std::process::ExitStatus {
    ///     Command::new("ls")
    ///         .status()
    ///         .await
    ///         .expect("ls command failed to run")
    /// }
    pub fn status(&mut self) -> impl Future<Output = io::Result<ExitStatus>> {
        let child = self.spawn();

        async {
            let mut child = child?;

            // Ensure we close any stdio handles so we can't deadlock
            // waiting on the child which may be waiting to read/write
            // to a pipe we're holding.
            child.stdin.take();
            child.stdout.take();
            child.stderr.take();

            child.await
        }
    }

    /// Executes the command as a child process, waiting for it to finish and
    /// collecting all of its output.
    ///
    /// > **Note**: this method, unlike the standard library, will
    /// > unconditionally configure the stdout/stderr handles to be pipes, even
    /// > if they have been previously configured. If this is not desired then
    /// > the `spawn` method should be used in combination with the
    /// > `wait_with_output` method on child.
    ///
    /// This method will return a future representing the collection of the
    /// child process's stdout/stderr. It will resolve to
    /// the `Output` type in the standard library, containing `stdout` and
    /// `stderr` as `Vec<u8>` along with an `ExitStatus` representing how the
    /// process exited.
    ///
    /// All I/O this child does will be associated with the current default
    /// event loop.
    ///
    /// If this future is dropped before the future resolves, then
    /// the child will be killed, if it was spawned.
    ///
    /// # Examples
    ///
    /// Basic usage:
    ///
    /// ```no_run
    /// use tokio::process::Command;
    ///
    /// async fn run_ls() {
    ///     let output: std::process::Output = Command::new("ls")
    ///         .output()
    ///         .await
    ///         .expect("ls command failed to run");
    ///     println!("stderr of ls: {:?}", output.stderr);
    /// }
    pub fn output(&mut self) -> impl Future<Output = io::Result<Output>> {
        self.std.stdout(Stdio::piped());
        self.std.stderr(Stdio::piped());

        let child = self.spawn();

        async { child?.wait_with_output().await }
    }
}

impl From<StdCommand> for Command {
    fn from(std: StdCommand) -> Command {
        Command {
            std,
            kill_on_drop: false,
        }
    }
}

/// A drop guard which can ensure the child process is killed on drop if specified.
#[derive(Debug)]
struct ChildDropGuard<T: Kill> {
    inner: T,
    kill_on_drop: bool,
}

impl<T: Kill> Kill for ChildDropGuard<T> {
    fn kill(&mut self) -> io::Result<()> {
        let ret = self.inner.kill();

        if ret.is_ok() {
            self.kill_on_drop = false;
        }

        ret
    }
}

impl<T: Kill> Drop for ChildDropGuard<T> {
    fn drop(&mut self) {
        if self.kill_on_drop {
            drop(self.kill());
        }
    }
}

impl<T, E, F> Future for ChildDropGuard<F>
where
    F: Future<Output = Result<T, E>> + Kill + Unpin,
{
    type Output = Result<T, E>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Keep track of task budget
        ready!(crate::coop::poll_proceed(cx));

        let ret = Pin::new(&mut self.inner).poll(cx);

        if let Poll::Ready(Ok(_)) = ret {
            // Avoid the overhead of trying to kill a reaped process
            self.kill_on_drop = false;
        }

        ret
    }
}

/// Representation of a child process spawned onto an event loop.
///
/// This type is also a future which will yield the `ExitStatus` of the
/// underlying child process. A `Child` here also provides access to information
/// like the OS-assigned identifier and the stdio streams.
///
/// # Caveats
/// Similar to the behavior to the standard library, and unlike the futures
/// paradigm of dropping-implies-cancellation, a spawned process will, by
/// default, continue to execute even after the `Child` handle has been dropped.
///
/// The `Command::kill_on_drop` method can be used to modify this behavior
/// and kill the child process if the `Child` wrapper is dropped before it
/// has exited.
#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub struct Child {
    child: ChildDropGuard<imp::Child>,

    /// The handle for writing to the child's standard input (stdin), if it has
    /// been captured.
    pub stdin: Option<ChildStdin>,

    /// The handle for reading from the child's standard output (stdout), if it
    /// has been captured.
    pub stdout: Option<ChildStdout>,

    /// The handle for reading from the child's standard error (stderr), if it
    /// has been captured.
    pub stderr: Option<ChildStderr>,
}

impl Child {
    /// Returns the OS-assigned process identifier associated with this child.
    pub fn id(&self) -> u32 {
        self.child.inner.id()
    }

    /// Forces the child to exit.
    ///
    /// This is equivalent to sending a SIGKILL on unix platforms.
    pub fn kill(&mut self) -> io::Result<()> {
        self.child.kill()
    }

    #[doc(hidden)]
    #[deprecated(note = "please use `child.stdin` instead")]
    pub fn stdin(&mut self) -> &mut Option<ChildStdin> {
        &mut self.stdin
    }

    #[doc(hidden)]
    #[deprecated(note = "please use `child.stdout` instead")]
    pub fn stdout(&mut self) -> &mut Option<ChildStdout> {
        &mut self.stdout
    }

    #[doc(hidden)]
    #[deprecated(note = "please use `child.stderr` instead")]
    pub fn stderr(&mut self) -> &mut Option<ChildStderr> {
        &mut self.stderr
    }

    /// Returns a future that will resolve to an `Output`, containing the exit
    /// status, stdout, and stderr of the child process.
    ///
    /// The returned future will simultaneously waits for the child to exit and
    /// collect all remaining output on the stdout/stderr handles, returning an
    /// `Output` instance.
    ///
    /// The stdin handle to the child process, if any, will be closed before
    /// waiting. This helps avoid deadlock: it ensures that the child does not
    /// block waiting for input from the parent, while the parent waits for the
    /// child to exit.
    ///
    /// By default, stdin, stdout and stderr are inherited from the parent. In
    /// order to capture the output into this `Output` it is necessary to create
    /// new pipes between parent and child. Use `stdout(Stdio::piped())` or
    /// `stderr(Stdio::piped())`, respectively, when creating a `Command`.
    pub async fn wait_with_output(mut self) -> io::Result<Output> {
        use crate::future::try_join3;

        async fn read_to_end<A: AsyncRead + Unpin>(io: Option<A>) -> io::Result<Vec<u8>> {
            let mut vec = Vec::new();
            if let Some(mut io) = io {
                crate::io::util::read_to_end(&mut io, &mut vec).await?;
            }
            Ok(vec)
        }

        drop(self.stdin.take());
        let stdout_fut = read_to_end(self.stdout.take());
        let stderr_fut = read_to_end(self.stderr.take());

        let (status, stdout, stderr) = try_join3(self, stdout_fut, stderr_fut).await?;

        Ok(Output {
            status,
            stdout,
            stderr,
        })
    }
}

impl Future for Child {
    type Output = io::Result<ExitStatus>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.child).poll(cx)
    }
}

/// The standard input stream for spawned children.
///
/// This type implements the `AsyncWrite` trait to pass data to the stdin handle of
/// handle of a child process asynchronously.
#[derive(Debug)]
pub struct ChildStdin {
    inner: imp::ChildStdin,
}

/// The standard output stream for spawned children.
///
/// This type implements the `AsyncRead` trait to read data from the stdout
/// handle of a child process asynchronously.
#[derive(Debug)]
pub struct ChildStdout {
    inner: imp::ChildStdout,
}

/// The standard error stream for spawned children.
///
/// This type implements the `AsyncRead` trait to read data from the stderr
/// handle of a child process asynchronously.
#[derive(Debug)]
pub struct ChildStderr {
    inner: imp::ChildStderr,
}

impl AsyncWrite for ChildStdin {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }
}

impl AsyncRead for ChildStdout {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl AsyncRead for ChildStderr {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

#[cfg(unix)]
mod sys {
    use std::os::unix::io::{AsRawFd, RawFd};

    use super::{ChildStderr, ChildStdin, ChildStdout};

    impl AsRawFd for ChildStdin {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.get_ref().as_raw_fd()
        }
    }

    impl AsRawFd for ChildStdout {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.get_ref().as_raw_fd()
        }
    }

    impl AsRawFd for ChildStderr {
        fn as_raw_fd(&self) -> RawFd {
            self.inner.get_ref().as_raw_fd()
        }
    }
}

#[cfg(windows)]
mod sys {
    use std::os::windows::io::{AsRawHandle, RawHandle};

    use super::{ChildStderr, ChildStdin, ChildStdout};

    impl AsRawHandle for ChildStdin {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.get_ref().as_raw_handle()
        }
    }

    impl AsRawHandle for ChildStdout {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.get_ref().as_raw_handle()
        }
    }

    impl AsRawHandle for ChildStderr {
        fn as_raw_handle(&self) -> RawHandle {
            self.inner.get_ref().as_raw_handle()
        }
    }
}

#[cfg(all(test, not(loom)))]
mod test {
    use super::kill::Kill;
    use super::ChildDropGuard;

    use futures::future::FutureExt;
    use std::future::Future;
    use std::io;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    struct Mock {
        num_kills: usize,
        num_polls: usize,
        poll_result: Poll<Result<(), ()>>,
    }

    impl Mock {
        fn new() -> Self {
            Self::with_result(Poll::Pending)
        }

        fn with_result(result: Poll<Result<(), ()>>) -> Self {
            Self {
                num_kills: 0,
                num_polls: 0,
                poll_result: result,
            }
        }
    }

    impl Kill for Mock {
        fn kill(&mut self) -> io::Result<()> {
            self.num_kills += 1;
            Ok(())
        }
    }

    impl Future for Mock {
        type Output = Result<(), ()>;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            let inner = Pin::get_mut(self);
            inner.num_polls += 1;
            inner.poll_result
        }
    }

    #[test]
    fn kills_on_drop_if_specified() {
        let mut mock = Mock::new();

        {
            let guard = ChildDropGuard {
                inner: &mut mock,
                kill_on_drop: true,
            };
            drop(guard);
        }

        assert_eq!(1, mock.num_kills);
        assert_eq!(0, mock.num_polls);
    }

    #[test]
    fn no_kill_on_drop_by_default() {
        let mut mock = Mock::new();

        {
            let guard = ChildDropGuard {
                inner: &mut mock,
                kill_on_drop: false,
            };
            drop(guard);
        }

        assert_eq!(0, mock.num_kills);
        assert_eq!(0, mock.num_polls);
    }

    #[test]
    fn no_kill_if_already_killed() {
        let mut mock = Mock::new();

        {
            let mut guard = ChildDropGuard {
                inner: &mut mock,
                kill_on_drop: true,
            };
            let _ = guard.kill();
            drop(guard);
        }

        assert_eq!(1, mock.num_kills);
        assert_eq!(0, mock.num_polls);
    }

    #[test]
    fn no_kill_if_reaped() {
        let mut mock_pending = Mock::with_result(Poll::Pending);
        let mut mock_reaped = Mock::with_result(Poll::Ready(Ok(())));
        let mut mock_err = Mock::with_result(Poll::Ready(Err(())));

        let waker = futures::task::noop_waker();
        let mut context = Context::from_waker(&waker);
        {
            let mut guard = ChildDropGuard {
                inner: &mut mock_pending,
                kill_on_drop: true,
            };
            let _ = guard.poll_unpin(&mut context);

            let mut guard = ChildDropGuard {
                inner: &mut mock_reaped,
                kill_on_drop: true,
            };
            let _ = guard.poll_unpin(&mut context);

            let mut guard = ChildDropGuard {
                inner: &mut mock_err,
                kill_on_drop: true,
            };
            let _ = guard.poll_unpin(&mut context);
        }

        assert_eq!(1, mock_pending.num_kills);
        assert_eq!(1, mock_pending.num_polls);

        assert_eq!(0, mock_reaped.num_kills);
        assert_eq!(1, mock_reaped.num_polls);

        assert_eq!(1, mock_err.num_kills);
        assert_eq!(1, mock_err.num_polls);
    }
}
