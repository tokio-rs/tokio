#![deny(warnings, rust_2018_idioms)]
#![feature(await_macro, async_await)]

use tokio_executor::{self, DefaultExecutor};

use std::future::Future;

mod out_of_executor_context {
    use super::*;
    use tokio_executor::Executor;

    fn test<F, E>(spawn: F)
    where
        F: Fn(Box<dyn Future<Output = ()> + Send>) -> Result<(), E>,
    {
        let res = spawn(Box::new(async { () }));
        assert!(res.is_err());
    }

    #[test]
    fn spawn() {
        test(|f| DefaultExecutor::current().spawn(f));
    }
}
