#![warn(rust_2018_idioms)]
#![feature(async_await)]

use tokio_executor::{self, DefaultExecutor};

use std::future::Future;
use std::pin::Pin;

mod out_of_executor_context {
    use super::*;
    use tokio_executor::Executor;

    fn test<F, E>(spawn: F)
    where
        F: Fn(Pin<Box<dyn Future<Output = ()> + Send>>) -> Result<(), E>,
    {
        let res = spawn(Box::pin(async {}));
        assert!(res.is_err());
    }

    #[test]
    fn spawn() {
        test(|f| DefaultExecutor::current().spawn(f));
    }
}
