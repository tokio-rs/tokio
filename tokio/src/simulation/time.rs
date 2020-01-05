//! Simulation Time
use crate::park::Park;
use std::{sync, time};
#[derive(Debug)]
pub(crate) struct SimTime {
    base: time::Instant,
    advance: time::Duration,
}

impl SimTime {
    pub(crate) fn new() -> Self {
        SimTime {
            base: time::Instant::now(),
            advance: time::Duration::from_millis(0),
        }
    }

    fn advance(&mut self, duration: time::Duration) {
        self.advance += duration;
    }

    pub(crate) fn now(&self) -> crate::time::Instant {
        crate::time::Instant::from_std(self.base + self.advance)
    }
}

#[derive(Debug)]
pub(crate) struct SimPark<P>
where
    P: Park + Send + 'static,
{
    inner: P,
    state: sync::Arc<sync::Mutex<super::State>>,
}

impl<P> SimPark<P>
where
    P: Park + Send + 'static,
{
    pub(super) fn new(inner: P, state: sync::Arc<sync::Mutex<super::State>>) -> Self {
        Self { inner, state }
    }
}

impl<P> Park for SimPark<P>
where
    P: Park + Send + 'static,
{
    type Unpark = P::Unpark;
    type Error = P::Error;
    fn unpark(&self) -> Self::Unpark {
        self.inner.unpark()
    }
    fn park(&mut self) -> Result<(), Self::Error> {
        self.inner.park()
    }
    fn park_timeout(&mut self, duration: time::Duration) -> Result<(), Self::Error> {
        let mut lock = self.state.lock().unwrap();
        lock.time.advance(duration);
        self.inner.park_timeout(time::Duration::from_millis(0))
    }
}
