use runtime::Inner;

use std::fmt;

use futures::{Future, Poll};

/// A future that resolves when the Tokio `Runtime` is shut down.
pub struct Shutdown {
    pub(super) inner: Box<Future<Item = (), Error = ()> + Send>,
}

impl Shutdown {
    pub(super) fn shutdown_now(inner: Inner) -> Self {
        let inner = Box::new({
            let pool = inner.pool;
            let reactor = inner.reactor;

            pool.shutdown_now().and_then(|_| {
                reactor.shutdown_now()
                    .then(|_| {
                        Ok(())
                    })
            })
        });

        Shutdown { inner }
    }
}

impl Future for Shutdown {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        try_ready!(self.inner.poll());
        Ok(().into())
    }
}

impl fmt::Debug for Shutdown {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("Shutdown")
            .field("inner", &"Box<Future<Item = (), Error = ()>>")
            .finish()
    }
}
