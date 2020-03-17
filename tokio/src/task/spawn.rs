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
    /// There is no guarantee that a spawned task will execute to completion.
    /// When a runtime is shutdown, all outstanding tasks are dropped,
    /// regardless of the lifecycle of that task.
    ///
    /// This function must be called from the context of a Tokio runtime. Tasks running on
    /// the Tokio runtime are always inside its context, but you can also enter the context
    /// using the [`Handle::enter`](crate::runtime::Handle::enter()) method.
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
    ///
    /// # Using `!Send` values from a task
    ///
    /// The task supplied to `spawn` must implement `Send`. However, it is
    /// possible to **use** `!Send` values from the task as long as they only
    /// exist between calls to `.await`.
    ///
    /// For example, this will work:
    ///
    /// ```
    /// use tokio::task;
    ///
    /// use std::rc::Rc;
    ///
    /// fn use_rc(rc: Rc<()>) {
    ///     // Do stuff w/ rc
    /// # drop(rc);
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     tokio::spawn(async {
    ///         // Force the `Rc` to stay in a scope with no `.await`
    ///         {
    ///             let rc = Rc::new(());
    ///             use_rc(rc.clone());
    ///         }
    ///
    ///         task::yield_now().await;
    ///     }).await.unwrap();
    /// }
    /// ```
    ///
    /// This will **not** work:
    ///
    /// ```compile_fail
    /// use tokio::task;
    ///
    /// use std::rc::Rc;
    ///
    /// fn use_rc(rc: Rc<()>) {
    ///     // Do stuff w/ rc
    /// # drop(rc);
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     tokio::spawn(async {
    ///         let rc = Rc::new(());
    ///
    ///         task::yield_now().await;
    ///
    ///         use_rc(rc.clone());
    ///     }).await.unwrap();
    /// }
    /// ```
    ///
    /// Holding on to a `!Send` value across calls to `.await` will result in
    /// an unfriendly compile error message similar to:
    ///
    /// ```text
    /// `[... some type ...]` cannot be sent between threads safely
    /// ```
    ///
    /// or:
    ///
    /// ```text
    /// error[E0391]: cycle detected when processing `main`
    /// ```
    pub fn spawn<T>(task: T) -> JoinHandle<T::Output>
    where
        T: Future + Send + 'static,
        T::Output: Send + 'static,
    {
        let spawn_handle = runtime::context::spawn_handle()
        .expect("must be called from the context of Tokio runtime configured with either `basic_scheduler` or `threaded_scheduler`");
        spawn_handle.spawn(task)
    }
}
