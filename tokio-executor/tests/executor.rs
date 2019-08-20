extern crate futures;
extern crate tokio_executor;

use futures::{future::lazy, Future};
use tokio_executor::DefaultExecutor;

mod out_of_executor_context {
    use super::*;
    use tokio_executor::Executor;

    fn test<F, E>(spawn: F)
    where
        F: Fn(Box<dyn Future<Item = (), Error = ()> + Send>) -> Result<(), E>,
    {
        let res = spawn(Box::new(lazy(|| Ok(()))));
        assert!(res.is_err());
    }

    #[test]
    fn spawn() {
        test(|f| DefaultExecutor::current().spawn(f));
    }

    #[test]
    fn execute() {
        use futures::future::Executor as FuturesExecutor;
        test(|f| DefaultExecutor::current().execute(f));
    }
}
