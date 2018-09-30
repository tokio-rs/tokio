use std::future::{Future as StdFuture};

async fn map_ok<T: StdFuture>(future: T) -> Result<(), ()> {
    let _ = await!(future);
    Ok(())
}

/// Like `tokio::run`, but takes an `async` block
pub fn run_async<F>(future: F)
where F: StdFuture<Output = ()> + Send + 'static,
{
    use tokio_async_await::compat::backward;
    let future = backward::Compat::new(map_ok(future));

    ::run(future);
}

/// Like `tokio::spawn`, but takes an `async` block
pub fn spawn_async<F>(future: F)
where F: StdFuture<Output = ()> + Send + 'static,
{
    use tokio_async_await::compat::backward;
    let future = backward::Compat::new(map_ok(future));

    ::spawn(future);
}
