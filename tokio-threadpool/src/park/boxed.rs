use tokio_executor::park::{Park, Unpark};

use std::time::Duration;

pub(crate) type BoxPark = Box<Park<Unpark = BoxUnpark, Error = ()> + Send>;
pub(crate) type BoxUnpark = Box<Unpark>;

pub(crate) struct Boxed<T>(T);

impl<T> Boxed<T> {
    pub fn new(inner: T) -> Self {
        Boxed(inner)
    }
}

impl<T: Park + Send> Park for Boxed<T> {
    type Unpark = BoxUnpark;
    type Error = ();

    fn unpark(&self) -> Self::Unpark {
        Box::new(self.0.unpark())
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        self.0.park()
            .map_err(|_| {
                // TODO: log error
                ()
            })
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.0.park_timeout(duration)
            .map_err(|_| {
                // TODO: log error
                ()
            })
    }
}
