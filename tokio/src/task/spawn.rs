use crate::runtime;
use crate::task::JoinHandle;

use std::future::Future;

doc_rt_core! {
    /// Spawns a new asynchronous task, returning a
    /// [`JoinHandle`](super::JoinHandle) for it.
    ///
    /// Spawning a task enables the task to execute concurrently to other tasks. The
    /// spawned task may execute on the current thread, or it may be sent to a
    /// different thread to be executed. The specifics depend on the current
    /// [`Runtime`](crate::runtime::Runtime) configuration.
    ///
    /// # Examples
    ///
    /// In this example, a server is started and `spawn` is used to start a new task
    /// that processes each received connection.
    ///
    /// ```no_run
    /// use tokio::net::{TcpListener, TcpStream};
    ///
    /// use std::io;
    ///
    /// async fn process(socket: TcpStream) {
    ///     // ...
    /// # drop(socket);
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let mut listener = TcpListener::bind("127.0.0.1:8080").await?;
    ///
    ///     loop {
    ///         let (socket, _) = listener.accept().await?;
    ///
    ///         tokio::spawn(async move {
    ///             // Process each socket concurrently.
    ///             process(socket).await
    ///         });
    ///     }
    /// }
    /// ```
    ///
    /// # Panics
    ///
    /// Panics if called from **outside** of the Tokio runtime.
    pub fn spawn<T>(task: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        runtime::spawn(task)
    }
}
