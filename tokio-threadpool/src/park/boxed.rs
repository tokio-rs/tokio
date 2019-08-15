use tokio_executor::park::{Park, Unpark};

use std::error::Error;
use std::time::Duration;

pub(crate) type BoxPark = Box<dyn Park<Unpark = BoxUnpark, Error = ()> + Send>;
pub(crate) type BoxUnpark = Box<dyn Unpark>;

pub(crate) struct BoxedPark<T>(T);

impl<T> BoxedPark<T> {
    pub fn new(inner: T) -> Self {
        BoxedPark(inner)
    }
}

impl<T: Park + Send> Park for BoxedPark<T>
where
    T::Error: Error,
{
    type Unpark = BoxUnpark;
    type Error = ();

    fn unpark(&self) -> Self::Unpark {
        Box::new(self.0.unpark())
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.0.park().map_err(|e| {
            warn!(
                "calling `park` on worker thread errored -- shutting down thread: {}",
                e
            );
        })
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.0.park_timeout(duration).map_err(|e| {
            warn!(
                "calling `park` on worker thread errored -- shutting down thread: {}",
                e
            );
        })
    }
}
