use tokio_executor_01::{self as executor_01, park as park_01};
use tokio_reactor_01 as reactor_01;
use tokio_timer_02::{clock as clock_02, timer as timer_02};

use std::{
    io, thread,
    time::{Duration, Instant},
};
use tokio_02::executor::{current_thread::CurrentThread, park};
use tokio_02::sync::oneshot;

#[derive(Debug)]
pub(super) struct Background {
    reactor_handle: reactor_01::Handle,
    timer_handle: timer_02::Handle,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

#[derive(Debug)]
pub(super) struct Now<N>(N);

#[derive(Debug)]
struct CompatPark<P>(P);

impl Background {
    pub(super) fn spawn(clock: &tokio_02::timer::clock::Clock) -> io::Result<Self> {
        let clock = clock_02::Clock::new_with_now(Now(clock.clone()));

        let reactor = reactor_01::Reactor::new()?;
        let reactor_handle = reactor.handle();

        let timer = timer_02::Timer::new_with_now(reactor, clock);
        let timer_handle = timer.handle();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_tx = Some(shutdown_tx);

        let thread = thread::spawn(move || {
            let mut rt = CurrentThread::new_with_park(CompatPark(timer));
            let _ = rt.block_on(shutdown_rx);
        });
        let thread = Some(thread);

        Ok(Self {
            reactor_handle,
            timer_handle,
            thread,
            shutdown_tx,
        })
    }

    pub(super) fn reactor(&self) -> &reactor_01::Handle {
        &self.reactor_handle
    }

    pub(super) fn timer(&self) -> &timer_02::Handle {
        &self.timer_handle
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.take().unwrap().send(());
        let _ = self.thread.take().unwrap().join();
    }
}

pub(super) fn spawn_err(new: tokio_02::executor::SpawnError) -> executor_01::SpawnError {
    match new {
        _ if new.is_shutdown() => executor_01::SpawnError::shutdown(),
        _ if new.is_at_capacity() => executor_01::SpawnError::at_capacity(),
        e => unreachable!("weird spawn error {:?}", e),
    }
}

impl<P> park::Park for CompatPark<P>
where
    P: park_01::Park,
{
    type Unpark = CompatPark<P::Unpark>;
    type Error = P::Error;

    fn unpark(&self) -> Self::Unpark {
        CompatPark(self.0.unpark())
    }

    #[inline]
    fn park(&mut self) -> Result<(), Self::Error> {
        self.0.park()
    }

    #[inline]
    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.0.park_timeout(duration)
    }
}

impl<U> park::Unpark for CompatPark<U>
where
    U: park_01::Unpark,
{
    #[inline]
    fn unpark(&self) {
        self.0.unpark()
    }
}

impl clock_02::Now for Now<tokio_02::timer::clock::Clock> {
    fn now(&self) -> Instant {
        self.0.now()
    }
}
