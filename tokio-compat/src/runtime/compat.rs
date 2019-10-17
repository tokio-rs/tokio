use tokio_executor_01::park as park_01;
use tokio_reactor_01 as reactor_01;
use tokio_timer_02::{clock as clock_02, timer as timer_02};

use std::{
    io, thread,
    time::{Duration, Instant},
};
use tokio_executor::{current_thread::CurrentThread, park};
use tokio_sync::oneshot;

#[derive(Debug)]
pub(super) struct Background {
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

pub(super) struct Compat {
    pub(super) compat_reactor: reactor_01::Handle,
    pub(super) compat_timer: timer_01::Handle,
    pub(super) compat_bg: Background,
}

impl Compat {
    pub(super) fn spawn(clock: &tokio_timer::clock::Clock) -> io::Result<Self> {
        let clock = clock_02::Clock::new_with_now(Now(clock.clone()));

        let reactor = reactor_01::Reactor::new()?;
        let reactor_handle = reactor.handle();

        let timer = timer_01::Timer::new_with_now(reactor, clock);
        let timer_handle = timer.handle();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let shutdown_tx = Some(shutdown_tx);

        let thread = thread::spawn(move || {
            let mut rt = CurrentThread::new_with_park(CompatPark(timer));
            let _ = rt.block_on(shutdown_rx);
        });
        let thread = Some(thread);

        Ok(Self {
            compat_reactor: reactor_handle,
            compat_timer: timer_handle,
            compat_bg: Background {
                thread,
                shutdown_tx,
            },
        })
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.take().unwrap().send(());
        let _ = self.thread.take().unwrap().join();
    }
}

#[derive(Debug)]
pub(super) struct Now<N>(N);

#[derive(Debug)]
struct CompatPark<P>(P);

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

impl timer_02::clock::Now for Now<tokio_timer::clock::Clock> {
    fn now(&self) -> Instant {
        self.0.now()
    }
}
