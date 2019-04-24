use tokio_futures::compat;

/// Like `tokio::run`, but takes an `async` block
pub fn run_async<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    ::run(compat::infallible_into_01(future));
}

/// Like `tokio::spawn`, but takes an `async` block
pub fn spawn_async<F>(future: F)
where
    F: std::future::Future<Output = ()> + Send + 'static,
{
    ::spawn(compat::infallible_into_01(future));
}
