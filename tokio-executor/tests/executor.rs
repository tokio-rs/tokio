extern crate futures;
extern crate tokio_executor;

use futures::{future::lazy, Future};
use futures::future::ok;
use tokio_executor::*;

mod out_of_executor_context {
    use super::*;

    fn test<F, E>(spawn: F)
    where
        F: Fn(Box<Future<Item = (), Error = ()> + Send>) -> Result<(), E>,
    {
        let res = spawn(Box::new(lazy(|| Ok(()))));
        assert!(res.is_err());
    }

    #[test]
    fn spawn() {
        test(|f| DefaultExecutor::current().spawn(f));
    }

    #[test]
    fn spawn_lazy() {
        let res = DefaultExecutor::current().spawn_lazy((move || Box::new(ok(())) as Box<Future<Item = (), Error = ()>>).into());
        assert!(res.is_err());
    }

    #[test]
    fn execute() {
        use futures::future::Executor as FuturesExecutor;
        test(|f| DefaultExecutor::current().execute(f));
    }
}
