extern crate tokio_executor;
extern crate futures;

use tokio_executor::*;
use futures::future::lazy;

#[test]
fn spawn_out_of_executor_context() {
    let res = DefaultExecutor::current().spawn(Box::new(lazy(|| Ok(()))));
    assert!(res.is_err());
}
