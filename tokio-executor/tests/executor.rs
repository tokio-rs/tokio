extern crate tokio_executor;
extern crate futures;

use tokio_executor::*;
use futures::future::lazy;

#[test]
fn spawn_out_of_executor_context() {
    let res = DefaultExecutor::current().spawn(Box::new(lazy(|| Ok(()))));
    assert!(res.is_err());
}

#[test]
fn spawn_out_of_executor_context_futures_executor() {
    use futures::future::Executor as FuturesExecutor;
    let res = DefaultExecutor::current().execute(lazy(|| Ok(())));
    assert!(res.is_err());
}
