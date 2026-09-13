// Signal handling
cfg_signal_internal_and_unix! {
    mod signal;
}
cfg_io_uring! {
    mod uring;
    use uring::UringContext;
    use crate::sync::OnceCell;
}

use crate::io::interest::Interest;
use crate::io::ready::Ready;
use crate::loom::sync::Mutex;
use crate::runtime::driver;
use crate::runtime::io::registration_set;
use crate::runtime::io::{IoDriverMetrics, RegistrationSet, ScheduledIo};

use mio::event::Source;
use std::fmt;
use std::io;
use std::sync::Arc;
use std::time::Duration;

/// I/O driver, backed by Mio.
///
/// With several shards: one `Driver` per shard, all sharing one [`Handle`].
pub(crate) struct Driver {
    /// Which shard of the handle this driver polls.
    shard: usize,

    /// True when an event with the signal token is received
    signal_ready: bool,

    /// Reuse the `mio::Events` value across calls to poll.
    events: mio::Events,

    /// Buffer for polls that do not wait, if
    /// `Builder::max_io_events_per_busy_tick` is set.
    events_busy: Option<mio::Events>,

    /// The system event queue.
    poll: mio::Poll,
}

/// One `epoll` instance and the registrations that live on it.
pub(crate) struct Shard {
    /// Registers I/O resources.
    registry: mio::Registry,

    /// Tracks all registrations
    registrations: RegistrationSet,

    /// State that should be synchronized
    synced: Mutex<registration_set::Synced>,

    /// Used to wake up the reactor from a call to `turn`.
    /// Not supported on `Wasi` due to lack of threading support.
    #[cfg(not(target_os = "wasi"))]
    waker: mio::Waker,

    /// When this shard's `epoll_wait` last returned (`poll_clock_us`); 0
    /// means never.
    polled_us: crate::loom::sync::atomic::AtomicU64,

    /// Whether this shard's last `epoll_wait` filled its event buffer, which
    /// means more readiness is waiting in the kernel.
    saturated: crate::loom::sync::atomic::AtomicBool,

    /// The next poll that does not wait takes the full batch instead of the
    /// busy batch. Set by `shard_request_full` and consumed in `turn`, both
    /// under the shard's driver lock.
    full_next: crate::loom::sync::atomic::AtomicBool,
}

/// A reference to an I/O driver (all shards).
pub(crate) struct Handle {
    shards: Box<[Shard]>,

    /// Round-robin cursor for new sources.
    next_shard: std::sync::atomic::AtomicUsize,

    pub(crate) metrics: IoDriverMetrics,

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux",
    ))]
    pub(crate) uring_context: Mutex<UringContext>,

    #[cfg(all(
        tokio_unstable,
        feature = "io-uring",
        feature = "rt",
        feature = "fs",
        target_os = "linux",
    ))]
    pub(crate) uring_probe: OnceCell<Option<io_uring::Probe>>,
}

#[derive(Debug)]
pub(crate) struct ReadyEvent {
    pub(super) tick: u16,
    pub(crate) ready: Ready,
    pub(super) is_shutdown: bool,
}

cfg_net_unix!(
    impl ReadyEvent {
        pub(crate) fn with_ready(&self, ready: Ready) -> Self {
            Self {
                ready,
                tick: self.tick,
                is_shutdown: self.is_shutdown,
            }
        }
    }
);

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
pub(super) enum Direction {
    Read,
    Write,
}

pub(super) enum Tick {
    Set,
    Clear(u16),
}

const TOKEN_WAKEUP: mio::Token = mio::Token(0);
const TOKEN_SIGNAL: mio::Token = mio::Token(1);

fn _assert_kinds() {
    fn _assert<T: Send + Sync>() {}

    _assert::<Handle>();
}

// ===== impl Driver =====

impl Driver {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    #[cfg(test)]
    pub(crate) fn new(nevents: usize, nevents_busy: Option<usize>) -> io::Result<(Driver, Handle)> {
        let (mut drivers, handle) = Self::new_sharded(nevents, nevents_busy, 1)?;
        Ok((drivers.pop().unwrap(), handle))
    }

    /// Creates `num_shards` event loops (one `epoll` instance each) sharing one
    /// [`Handle`]. `drivers[i]` polls shard `i`.
    pub(crate) fn new_sharded(
        nevents: usize,
        nevents_busy: Option<usize>,
        num_shards: usize,
    ) -> io::Result<(Vec<Driver>, Handle)> {
        let num_shards = num_shards.max(1);
        let mut drivers = Vec::with_capacity(num_shards);
        let mut shards = Vec::with_capacity(num_shards);

        for shard in 0..num_shards {
            let poll = mio::Poll::new()?;
            #[cfg(not(target_os = "wasi"))]
            let waker = mio::Waker::new(poll.registry(), TOKEN_WAKEUP)?;
            let registry = poll.registry().try_clone()?;
            let (registrations, synced) = RegistrationSet::new();

            drivers.push(Driver {
                shard,
                signal_ready: false,
                events: mio::Events::with_capacity(nevents),
                events_busy: nevents_busy.map(mio::Events::with_capacity),
                poll,
            });
            shards.push(Shard {
                registry,
                registrations,
                synced: Mutex::new(synced),
                #[cfg(not(target_os = "wasi"))]
                waker,
                polled_us: crate::loom::sync::atomic::AtomicU64::new(0),
                saturated: crate::loom::sync::atomic::AtomicBool::new(false),
                full_next: crate::loom::sync::atomic::AtomicBool::new(false),
            });
        }

        let handle = Handle {
            shards: shards.into_boxed_slice(),
            next_shard: std::sync::atomic::AtomicUsize::new(0),
            metrics: IoDriverMetrics::default(),
            #[cfg(all(
                tokio_unstable,
                feature = "io-uring",
                feature = "rt",
                feature = "fs",
                target_os = "linux",
            ))]
            uring_context: Mutex::new(UringContext::new()),
            #[cfg(all(
                tokio_unstable,
                feature = "io-uring",
                feature = "rt",
                feature = "fs",
                target_os = "linux",
            ))]
            uring_probe: OnceCell::new(),
        };

        Ok((drivers, handle))
    }

    pub(crate) fn park(&mut self, rt_handle: &driver::Handle) {
        let handle = rt_handle.io();
        self.turn(handle, None);
    }

    pub(crate) fn park_timeout(&mut self, rt_handle: &driver::Handle, duration: Duration) {
        let handle = rt_handle.io();
        self.turn(handle, Some(duration));
    }

    pub(crate) fn shutdown(&mut self, rt_handle: &driver::Handle) {
        let shard = &rt_handle.io().shards[self.shard];
        let ios = shard.registrations.shutdown(&mut shard.synced.lock());

        // `shutdown()` must be called without holding the lock.
        for io in ios {
            io.shutdown();
        }
    }

    fn turn(&mut self, handle: &Handle, max_wait: Option<Duration>) {
        let shard = &handle.shards[self.shard];
        debug_assert!(!shard.registrations.is_shutdown(&shard.synced.lock()));

        shard.release_pending_registrations();

        // A poll that does not wait takes the busy batch. Events it leaves
        // behind stay queued in the kernel, so the next poll returns them.
        // The exception is the poll that follows `Handle::shard_request_full`.
        let full = shard
            .full_next
            .swap(false, std::sync::atomic::Ordering::Relaxed);
        let events = match (&mut self.events_busy, max_wait) {
            (Some(busy), Some(wait)) if wait.is_zero() && !full => busy,
            _ => &mut self.events,
        };

        // Block waiting for an event to happen, peeling out how many events
        // happened.
        let res = self.poll.poll(events, max_wait);
        shard
            .polled_us
            .store(poll_clock_us(), std::sync::atomic::Ordering::Relaxed);
        match res {
            Ok(()) => {}
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => {}
            #[cfg(target_os = "wasi")]
            Err(e) if e.kind() == io::ErrorKind::InvalidInput => {
                // In case of wasm32_wasi this error happens, when trying to poll without subscriptions
                // just return from the park, as there would be nothing, which wakes us up.
            }
            Err(e) => panic!("unexpected error when polling the I/O driver: {e:?}"),
        }

        // Process all the events that came in, dispatching appropriately
        let mut ready_count = 0;
        let mut event_count = 0;
        for event in events.iter() {
            event_count += 1;
            let token = event.token();

            if token == TOKEN_WAKEUP {
                // Nothing to do, the event is used to unblock the I/O driver
            } else if token == TOKEN_SIGNAL {
                self.signal_ready = true;
            } else {
                let ready = Ready::from_mio(event);
                let ptr = super::EXPOSE_IO.from_exposed_addr(token.0);

                // Safety: we ensure that the pointers used as tokens are not freed
                // until they are both deregistered from mio **and** we know the I/O
                // driver is not concurrently polling. The I/O driver holds ownership of
                // an `Arc<ScheduledIo>` so we can safely cast this to a ref.
                let io: &ScheduledIo = unsafe { &*ptr };

                io.set_readiness(Tick::Set, |curr| curr | ready);
                io.wake(ready);

                ready_count += 1;
            }
        }
        shard.saturated.store(
            event_count >= events.capacity(),
            std::sync::atomic::Ordering::Relaxed,
        );

        #[cfg(all(
            tokio_unstable,
            feature = "io-uring",
            feature = "rt",
            feature = "fs",
            target_os = "linux",
        ))]
        if self.shard == 0 {
            let mut guard = handle.get_uring().lock();
            let ctx = &mut *guard;
            ctx.dispatch_completions();

            // There might be some cases where the CQ overflows, so we need to flush
            // the remaining buffered CQEs.
            while ctx
                .uring
                .as_mut()
                .is_some_and(|uring| uring.submission().cq_overflow())
            {
                ctx.submit()
                    .expect("failed to flush io_uring completion queue overflow");
                ctx.dispatch_completions();
            }
        }

        handle.metrics.incr_ready_count_by(ready_count);
    }
}

impl fmt::Debug for Driver {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Driver")
    }
}

impl Handle {
    /// Whether shard `shard` returned from `epoll_wait` within `within`.
    pub(crate) fn shard_polled_within(&self, shard: usize, within: Duration) -> bool {
        let last = self.shards[shard]
            .polled_us
            .load(std::sync::atomic::Ordering::Relaxed);
        last != 0 && poll_clock_us().saturating_sub(last) < within.as_micros() as u64
    }

    /// Whether shard `shard`'s last `epoll_wait` filled its event buffer.
    pub(crate) fn shard_saturated(&self, shard: usize) -> bool {
        self.shards[shard]
            .saturated
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Makes the next poll of `shard` that does not wait take the full batch.
    /// The caller holds that shard's driver and polls it next.
    pub(crate) fn shard_request_full(&self, shard: usize) {
        self.shards[shard]
            .full_next
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Forces a reactor blocked in a call to `turn` to wakeup, or otherwise
    /// makes the next call to `turn` return immediately.
    ///
    /// This method is intended to be used in situations where a notification
    /// needs to otherwise be sent to the main reactor. If the reactor is
    /// currently blocked inside of `turn` then it will wake up and soon return
    /// after this method has been called. If the reactor is not currently
    /// blocked in `turn`, then the next call to `turn` will not block and
    /// return immediately.
    ///
    /// With several shards this wakes every shard's poller.
    pub(crate) fn unpark(&self) {
        #[cfg(not(target_os = "wasi"))]
        for shard in self.shards.iter() {
            shard.waker.wake().expect("failed to wake I/O driver");
        }
    }

    /// Wakes the poller of one shard.
    pub(crate) fn unpark_shard(&self, shard: usize) {
        #[cfg(not(target_os = "wasi"))]
        self.shards[shard]
            .waker
            .wake()
            .expect("failed to wake I/O driver");
        #[cfg(target_os = "wasi")]
        let _ = shard;
    }

    /// Registry of shard 0; used for the signal pipe and the `io_uring` `eventfd`.
    #[allow(dead_code)]
    pub(super) fn registry(&self) -> &mio::Registry {
        &self.shards[0].registry
    }

    /// Picks the shard for a new source, round-robin.
    fn pick_shard(&self) -> usize {
        let n = self.shards.len();
        if n == 1 {
            return 0;
        }
        self.next_shard
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            % n
    }

    /// Registers an I/O resource with the reactor for a given `mio::Ready` state.
    ///
    /// The registration token is returned.
    pub(super) fn add_source(
        &self,
        source: &mut impl mio::event::Source,
        interest: Interest,
    ) -> io::Result<Arc<ScheduledIo>> {
        let shard_idx = self.pick_shard();
        let shard = &self.shards[shard_idx];
        let scheduled_io = shard
            .registrations
            .allocate(&mut shard.synced.lock(), shard_idx)?;
        let token = scheduled_io.token();

        // we should remove the `scheduled_io` from the `registrations` set if registering
        // the `source` with the OS fails. Otherwise it will leak the `scheduled_io`.
        if let Err(e) = shard.registry.register(source, token, interest.to_mio()) {
            // safety: `scheduled_io` is part of the `registrations` set.
            unsafe {
                shard
                    .registrations
                    .remove(&mut shard.synced.lock(), &scheduled_io)
            };

            return Err(e);
        }

        // TODO: move this logic to `RegistrationSet` and use a `CountedLinkedList`
        self.metrics.incr_fd_count();

        Ok(scheduled_io)
    }

    /// Deregisters an I/O resource from the reactor.
    pub(super) fn deregister_source(
        &self,
        registration: &Arc<ScheduledIo>,
        source: &mut impl Source,
    ) -> io::Result<()> {
        let shard_idx = registration.shard();
        let shard = &self.shards[shard_idx];

        // Deregister the source with the OS poller **first**
        // Cleanup ALWAYS happens
        let os_result = shard.registry.deregister(source);

        if shard
            .registrations
            .deregister(&mut shard.synced.lock(), registration)
        {
            self.unpark_shard(shard_idx);
        }

        self.metrics.dec_fd_count();

        os_result // Return error after cleanup
    }
}

/// Microseconds since a process-wide origin; never 0.
fn poll_clock_us() -> u64 {
    static T: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    (T.get_or_init(std::time::Instant::now).elapsed().as_micros() as u64).max(1)
}

impl Shard {
    fn release_pending_registrations(&self) {
        if self.registrations.needs_release() {
            self.registrations.release(&mut self.synced.lock());
        }
    }
}

impl fmt::Debug for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Handle")
    }
}

impl Direction {
    pub(super) fn mask(self) -> Ready {
        match self {
            Direction::Read => Ready::READABLE | Ready::READ_CLOSED,
            Direction::Write => Ready::WRITABLE | Ready::WRITE_CLOSED,
        }
    }
}

#[cfg(all(test, unix, feature = "net", not(loom), not(miri)))]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn busy_turn_takes_busy_batch() {
        let (mut driver, handle) = Driver::new(16, Some(2)).unwrap();
        let mut sources = Vec::new();
        for _ in 0..5 {
            let (mut rx, mut tx) = mio::net::UnixStream::pair().unwrap();
            tx.write_all(b"x").unwrap();
            let reg = handle.add_source(&mut rx, Interest::READABLE).unwrap();
            sources.push((rx, tx, reg));
        }

        // A poll that does not wait takes the busy batch.
        driver.turn(&handle, Some(Duration::ZERO));
        assert_eq!(driver.events_busy.as_ref().unwrap().iter().count(), 2);

        // The rest stays queued for the next poll, which takes the main batch.
        driver.turn(&handle, Some(Duration::from_millis(100)));
        assert_eq!(driver.events.iter().count(), 3);

        for (mut rx, _tx, reg) in sources {
            handle.deregister_source(&reg, &mut rx).unwrap();
        }
        handle.shards[0].release_pending_registrations();
    }
}
