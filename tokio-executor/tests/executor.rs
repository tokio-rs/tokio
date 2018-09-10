extern crate tokio_executor;
extern crate futures;

use tokio_executor::*;
use futures::{Future, future::lazy};

mod out_of_executor_context {
    use super::*;

    fn test<F, E>(spawn: F)
    where
        F: Fn(Box<Future<Item=(), Error=()> + Send>) -> Result<(), E>,
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
