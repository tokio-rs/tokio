use std::marker::PhantomData;

use crate::runtime::Callback;

/// [`LocalRuntime`]-only config options
///
/// Use `LocalOptions::default()` to create the default set of options. This type is used with
/// [`Builder::build_local`].
///
/// When using [`Builder::build_local`], this overrides any pre-configured options set on the
/// [`Builder`].
///
/// [`Builder::build_local`]: crate::runtime::Builder::build_local
/// [`LocalRuntime`]: crate::runtime::LocalRuntime
/// [`Builder`]: crate::runtime::Builder
#[derive(Default)]
#[non_exhaustive]
#[allow(missing_debug_implementations)]
pub struct LocalOptions {
    /// Marker used to make this !Send and !Sync.
    _phantom: PhantomData<*mut u8>,

    /// To run before the local runtime is parked.
    pub(crate) before_park: Option<Callback>,

    /// To run before the local runtime is spawned.
    pub(crate) after_unpark: Option<Callback>,
}

impl std::fmt::Debug for LocalOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalOptions")
            .field("before_park", &self.before_park.as_ref().map(|_| "..."))
            .field("after_unpark", &self.after_unpark.as_ref().map(|_| "..."))
            .finish()
    }
}

impl LocalOptions {
    /// Executes function `f` just before the local runtime is parked (goes idle).
    /// `f` is called within the Tokio context, so functions like [`tokio::spawn`](crate::spawn)
    /// can be called, and may result in this thread being unparked immediately.
    ///
    /// This can be used to start work only when the executor is idle, or for bookkeeping
    /// and monitoring purposes.
    ///
    /// This differs from the [`Builder::on_thread_park`] method in that it accepts a non Send + Sync
    /// closure.
    ///
    /// Note: There can only be one park callback for a runtime; calling this function
    /// more than once replaces the last callback defined, rather than adding to it.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime::{Builder, LocalOptions};
    /// # pub fn main() {
    /// let (tx, rx) = std::sync::mpsc::channel();
    /// let mut opts = LocalOptions::default();
    /// opts.on_thread_park(move || match rx.recv() {
    ///    Ok(x) => println!("Received from channel: {}", x),
    ///    Err(e) => println!("Error receiving from channel: {}", e),
    /// });
    ///
    /// let runtime = Builder::new_current_thread()
    ///     .enable_time()
    ///     .build_local(opts)
    ///     .unwrap();
    ///
    /// runtime.block_on(async {
    ///     tokio::task::spawn_local(async move {
    ///         tx.send(42).unwrap();
    ///     });
    ///     tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    /// })
    /// # }
    /// ```
    ///
    /// [`Builder`]: crate::runtime::Builder
    /// [`Builder::on_thread_park`]: crate::runtime::Builder::on_thread_park
    pub fn on_thread_park<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + 'static,
    {
        self.before_park = Some(std::sync::Arc::new(to_send_sync(f)));
        self
    }

    /// Executes function `f` just after the local runtime unparks (starts executing tasks).
    ///
    /// This is intended for bookkeeping and monitoring use cases; note that work
    /// in this callback will increase latencies when the application has allowed one or
    /// more runtime threads to go idle.
    ///
    /// This differs from the [`Builder::on_thread_unpark`] method in that it accepts a non Send + Sync
    /// closure.
    ///
    /// Note: There can only be one unpark callback for a runtime; calling this function
    /// more than once replaces the last callback defined, rather than adding to it.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio::runtime::{Builder, LocalOptions};
    /// # pub fn main() {
    /// let (tx, rx) = std::sync::mpsc::channel();
    /// let mut opts = LocalOptions::default();
    /// opts.on_thread_unpark(move || match rx.recv() {
    ///    Ok(x) => println!("Received from channel: {}", x),
    ///    Err(e) => println!("Error receiving from channel: {}", e),
    /// });
    ///
    /// let runtime = Builder::new_current_thread()
    ///     .enable_time()
    ///     .build_local(opts)
    ///     .unwrap();
    ///
    /// runtime.block_on(async {
    ///     tokio::task::spawn_local(async move {
    ///         tx.send(42).unwrap();
    ///     });
    ///     tokio::time::sleep(std::time::Duration::from_millis(1)).await;
    /// })
    /// # }
    /// ```
    ///
    /// [`Builder`]: crate::runtime::Builder
    /// [`Builder::on_thread_unpark`]: crate::runtime::Builder::on_thread_unpark
    pub fn on_thread_unpark<F>(&mut self, f: F) -> &mut Self
    where
        F: Fn() + 'static,
    {
        self.after_unpark = Some(std::sync::Arc::new(to_send_sync(f)));
        self
    }
}

// A wrapper type to allow non-Send + Sync closures to be used in a Send + Sync context.
// This is specifically used for executing callbacks when using a `LocalRuntime`.
struct UnsafeSendSync<T>(T);

// SAFETY: This type is only used in a context where it is guaranteed that the closure will not be
// sent across threads.
unsafe impl<T> Send for UnsafeSendSync<T> {}
unsafe impl<T> Sync for UnsafeSendSync<T> {}

impl<T: Fn()> UnsafeSendSync<T> {
    fn call(&self) {
        (self.0)()
    }
}

fn to_send_sync<F>(f: F) -> impl Fn() + Send + Sync
where
    F: Fn(),
{
    let f = UnsafeSendSync(f);
    move || f.call()
}
