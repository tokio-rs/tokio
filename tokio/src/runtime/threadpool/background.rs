//! Temporary reactor + timer that runs on a background thread. This it to make
//! `block_on` work.

use tokio_executor::current_thread::CurrentThread;
use tokio_reactor::Reactor;
use tokio_sync::oneshot;
use tokio_timer::clock::Clock;
use tokio_timer::timer::{self, Timer};

use std::{io, thread};

#[derive(Debug)]
pub(crate) struct Background {
    reactor_handle: tokio_reactor::Handle,
    timer_handle: timer::Handle,
    shutdown_tx: Option<oneshot::Sender<()>>,
    thread: Option<thread::JoinHandle<()>>,
}

pub(crate) fn spawn(clock: &Clock) -> io::Result<Background> {
    let clock = clock.clone();

    let reactor = Reactor::new()?;
    let reactor_handle = reactor.handle();

    let timer = Timer::new_with_now(reactor, clock);
    let timer_handle = timer.handle();

    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let shutdown_tx = Some(shutdown_tx);

    let thread = thread::spawn(move || {
        let mut rt = CurrentThread::new_with_park(timer);
        let _ = rt.block_on(shutdown_rx);
    });
    let thread = Some(thread);

    Ok(Background {
        reactor_handle,
        timer_handle,
        shutdown_tx,
        thread,
    })
}

impl Background {
    pub(super) fn reactor(&self) -> &tokio_reactor::Handle {
        &self.reactor_handle
    }

    pub(super) fn timer(&self) -> &timer::Handle {
        &self.timer_handle
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.take().unwrap().send(());
        let _ = self.thread.take().unwrap().join();
    }
}
