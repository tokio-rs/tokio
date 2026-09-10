//! Parks the runtime.
//!
//! A combination of the various resource driver park handles.

use crate::loom::sync::atomic::{AtomicBool, AtomicUsize};
use crate::loom::sync::{Arc, Condvar, Mutex};
use crate::runtime::driver::{self, Driver};
use crate::util::TryLock;

use std::sync::atomic::Ordering::SeqCst;
use std::time::{Duration, Instant};

#[cfg(loom)]
use crate::runtime::park::CURRENT_THREAD_PARK_COUNT;

pub(crate) struct Parker {
    inner: Arc<Inner>,

    /// When a pre-park sweep by this worker last filled every batch; see
    /// `sweep`.
    last_full_sweep: Option<Instant>,
}

pub(crate) struct Unparker {
    inner: Arc<Inner>,
}

/// Represents how a worker thread was parked
#[derive(Copy, Clone, Eq, PartialEq)]
pub(crate) enum HadDriver {
    Yes,
    No,
}

struct Inner {
    /// Avoids entering the park if possible
    state: AtomicUsize,

    /// I/O driver shard this parker's driver polls; used to target unparks.
    shard: usize,

    /// Drivers of every shard (including ours), for the 0-timeout help pass.
    all: Arc<HelpSet>,

    /// Used to coordinate access to the driver / `condvar`
    mutex: Mutex<()>,

    /// `Condvar` to block on if the driver is unavailable.
    condvar: Condvar,

    /// Resource (I/O, time, ...) driver
    shared: Arc<Shared>,
}

const EMPTY: usize = 0;
const PARKED_CONDVAR: usize = 1;
const PARKED_DRIVER: usize = 2;
const NOTIFIED: usize = 3;

/// Shared across multiple Parker handles
struct Shared {
    /// Shared driver. Only one thread at a time can use this
    driver: TryLock<Driver>,

    /// True while the holder of `driver` is blocked in it (`epoll_wait`)
    /// rather than making a zero-timeout poll. Set only with several shards.
    blocking: AtomicBool,

    /// Workers of this group that found `driver` taken and sleep on their
    /// condvar until `pass` wakes one; see `Inner::park`.
    standby: Mutex<Vec<Arc<Inner>>>,
}

impl Shared {
    /// After a zero-timeout poll: if no thread is blocking on this shard, wake
    /// one standby worker so that it takes the driver.
    fn pass_if_unwatched(&self) {
        if !self.blocking.load(SeqCst) {
            self.pass();
        }
    }

    /// Wakes one standby worker. The state swap is under the standby lock, so
    /// a worker that already left the list (`unpublish`) is not notified. A
    /// worker notified before it sleeps sees `NOTIFIED`, returns at once and
    /// tries the driver again.
    fn pass(&self) {
        let sb = &mut *self.standby.lock();
        if let Some(w) = sb.pop() {
            if w.state.swap(NOTIFIED, SeqCst) == PARKED_CONDVAR {
                w.unpark_condvar();
            }
        }
    }
}

/// Every shard's `Shared`, and the thresholds for polling another group's
/// shard; see `Inner::help`.
pub(crate) struct HelpSet {
    shards: Box<[Arc<Shared>]>,
    /// Longest a sharded driver blocks; `None` means no limit.
    sweep: Option<Duration>,
    /// A pre-park sweep skips a shard polled within this long.
    fresh: Duration,
    /// A maintenance tick polls only a shard not polled for this long. `None`
    /// when `sweep` is.
    backstop: Option<Duration>,
}

/// `HelpSet::backstop` in sweep intervals.
const HELP_BACKSTOP_SWEEPS: u32 = 10;

/// Lower bound on `HelpSet::fresh`. A busy owner polls its shard only at
/// maintenance ticks, so the gap between its polls can exceed a millisecond.
const HELP_FRESH: Duration = Duration::from_millis(1);

impl HelpSet {
    fn sharded(&self) -> bool {
        self.shards.len() > 1
    }
}

impl Parker {
    /// One parker per I/O shard; `parkers[i]` owns `drivers[i]`. Workers of
    /// group `i` are built from clones of `parkers[i]`.
    pub(crate) fn for_shards(drivers: Vec<Driver>, sweep: Option<Duration>) -> Vec<Parker> {
        let shareds: Box<[Arc<Shared>]> = drivers
            .into_iter()
            .map(|driver| {
                Arc::new(Shared {
                    driver: TryLock::new(driver),
                    blocking: AtomicBool::new(false),
                    standby: Mutex::new(Vec::new()),
                })
            })
            .collect();
        let all = Arc::new(HelpSet {
            sweep,
            fresh: sweep.map_or(HELP_FRESH, |d| (d / 2).max(HELP_FRESH)),
            backstop: sweep.map(|d| d.saturating_mul(HELP_BACKSTOP_SWEEPS)),
            shards: shareds.clone(),
        });
        shareds
            .iter()
            .enumerate()
            .map(|(shard, shared)| Parker {
                last_full_sweep: None,
                inner: Arc::new(Inner {
                    state: AtomicUsize::new(EMPTY),
                    shard,
                    all: all.clone(),
                    mutex: Mutex::new(()),
                    condvar: Condvar::new(),
                    shared: shared.clone(),
                }),
            })
            .collect()
    }

    pub(crate) fn unpark(&self) -> Unparker {
        Unparker {
            inner: self.inner.clone(),
        }
    }

    pub(crate) fn park(&mut self, handle: &driver::Handle) -> HadDriver {
        Inner::park(&self.inner, handle)
    }

    /// The worker is leaving its park loop. If that leaves no thread blocking
    /// on this shard, wake one standby worker to take over. A blocker that
    /// only wakes and parks again does not hand off.
    pub(crate) fn leave(&mut self) {
        let s = &self.inner.shared;
        if self.inner.all.sharded() && !s.blocking.load(SeqCst) && !s.driver.is_locked() {
            s.pass();
        }
    }

    /// Zero-timeout polls of the I/O shards before a park. Returns whether
    /// they woke tasks into this worker's queue (`has_tasks`); the caller then
    /// skips the park. A no-op with one shard.
    ///
    /// A maintenance tick (`tick`) only helps stranded shards; `park_timeout`
    /// polls our own. A real park polls our own shard and, if that woke
    /// nothing, helps the others. These polls take the busy batch, except:
    /// when every poll of this sweep filled its batch, and so did every poll
    /// of an earlier sweep of ours within `HelpSet::fresh`, our own shard is
    /// polled once more with the full batch. A single-shard runtime gets that
    /// batch from an idle worker's blocking poll; here the sweep keeps finding
    /// work, so workers rarely block. One saturated sweep is not enough: under
    /// request overload a worker that seldom idles meets one whenever it does,
    /// and a full batch there puts backlog the busy cap left in the kernel
    /// onto one busy worker.
    pub(crate) fn sweep(
        &mut self,
        handle: &driver::Handle,
        tick: bool,
        has_tasks: impl Fn() -> bool,
    ) -> bool {
        let this = &*self.inner;
        if !this.all.sharded() {
            return false;
        }
        if tick {
            this.help(handle, true);
            return false;
        }
        let own = this.poll(handle, this.shard, false);
        // Polls made by this sweep, and how many of them filled their batch.
        let mut polls = usize::from(own.is_some());
        let mut saturated = usize::from(own == Some(true));
        let mut woke = own.is_some() && has_tasks();
        if !woke {
            let (p, s) = this.help(handle, false);
            polls += p;
            saturated += s;
            woke = has_tasks();
        }
        if polls > 0 && saturated == polls {
            let now = Instant::now();
            let again = self
                .last_full_sweep
                .is_some_and(|t| now.saturating_duration_since(t) <= this.all.fresh);
            self.last_full_sweep = Some(now);
            if again && this.poll(handle, this.shard, true).is_some() {
                woke = woke || has_tasks();
            }
        }
        woke
    }

    /// Parks the current thread for up to `duration`.
    ///
    /// This function tries to acquire the driver lock. If it succeeds, it
    /// parks using the driver. Otherwise, it fails back to using a condvar,
    /// unless the duration is zero, in which case it returns immediately.
    pub(crate) fn park_timeout(
        &mut self,
        handle: &driver::Handle,
        duration: Duration,
    ) -> HadDriver {
        if let Some(mut driver) = self.inner.shared.driver.try_lock() {
            let r = self.inner.park_driver(&mut driver, handle, Some(duration));
            drop(driver);
            if duration.is_zero() && self.inner.all.sharded() {
                self.inner.shared.pass_if_unwatched();
            }
            r
        } else if !duration.is_zero() {
            // A timed park (alternative timer) waits like an untimed one: on
            // the standby list, so that it is woken to take the driver.
            let this = &self.inner;
            if this.all.sharded() {
                this.shared.standby.lock().push(this.clone());
                if let Some(mut driver) = this.shared.driver.try_lock() {
                    this.unpublish(this);
                    return this.park_driver(&mut driver, handle, Some(duration));
                }
            }
            this.park_condvar(Some(duration));
            this.unpublish(this);
            HadDriver::No
        } else {
            // https://github.com/tokio-rs/tokio/issues/6536
            // Hacky, but it's just for loom tests. The counter gets incremented during
            // `park_timeout`, but we still have to increment the counter if we can't acquire the
            // lock.
            #[cfg(loom)]
            CURRENT_THREAD_PARK_COUNT.with(|count| count.fetch_add(1, SeqCst));
            HadDriver::No
        }
    }

    pub(crate) fn shutdown(&mut self, handle: &driver::Handle) {
        self.inner.shutdown(handle);
    }
}

impl Clone for Parker {
    fn clone(&self) -> Parker {
        Parker {
            last_full_sweep: None,
            inner: Arc::new(Inner {
                state: AtomicUsize::new(EMPTY),
                shard: self.inner.shard,
                all: self.inner.all.clone(),
                mutex: Mutex::new(()),
                condvar: Condvar::new(),
                shared: self.inner.shared.clone(),
            }),
        }
    }
}

impl Unparker {
    pub(crate) fn unpark(&self, driver: &driver::Handle) {
        self.inner.unpark(driver);
    }
}

impl Inner {
    /// Parks the current thread for at most `dur`.
    fn park(this: &Arc<Inner>, handle: &driver::Handle) -> HadDriver {
        // If we were previously notified then we consume this notification and
        // return quickly.
        if this
            .state
            .compare_exchange(NOTIFIED, EMPTY, SeqCst, SeqCst)
            .is_ok()
        {
            return HadDriver::No;
        }

        if !this.all.sharded() {
            return if let Some(mut driver) = this.shared.driver.try_lock() {
                this.park_driver(&mut driver, handle, None)
            } else {
                this.park_condvar(None);
                HadDriver::No
            };
        }

        // Several shards: a shard must keep a thread in `epoll_wait` while any
        // of its group is idle. If the driver is taken (by a sibling blocking
        // in it, or briefly by a tick or a help sweep), go on the standby list,
        // re-check the lock in case its holder released it before we were
        // listed, and sleep. Whoever leaves the shard with no thread blocking
        // on it wakes one standby worker (`Shared::pass`).
        if let Some(mut driver) = this.shared.driver.try_lock() {
            return this.park_driver(&mut driver, handle, None);
        }
        this.shared.standby.lock().push(this.clone());
        if let Some(mut driver) = this.shared.driver.try_lock() {
            this.unpublish(this);
            return this.park_driver(&mut driver, handle, None);
        }
        this.park_condvar(None);
        this.unpublish(this);
        HadDriver::No
    }

    fn unpublish(&self, this: &Arc<Inner>) {
        let mut sb = self.shared.standby.lock();
        if let Some(pos) = sb.iter().position(|x| Arc::ptr_eq(x, this)) {
            sb.swap_remove(pos);
        }
    }

    /// Zero-timeout poll of the other shards that need it, so that a shard
    /// whose whole group is busy is still polled. Tasks woken land in this
    /// worker's queue. Before a park (`tick` false) that is a shard not polled
    /// within `fresh` (one polled more recently is being drained by its own
    /// group) or whose last poll filled its buffer. At a maintenance tick it
    /// is only a shard not polled for `backstop`: a busy worker that polls a
    /// shard its owners are draining moves that shard's tasks into a busy
    /// queue while the owners sleep. Only the I/O stack is polled; each
    /// shard's own parks keep the timer wheel current. The sweep starts after
    /// our shard so that groups do not all try shard 0 first.
    ///
    /// Returns `(polls, saturated)`: how many polls it made, and how many of
    /// them filled their batch.
    fn help(&self, handle: &driver::Handle, tick: bool) -> (usize, usize) {
        let (mut polls, mut saturated) = (0, 0);
        let n = self.all.shards.len();
        for i in 1..n {
            let index = (self.shard + i) % n;
            let skip = if tick {
                match self.all.backstop {
                    Some(b) => handle.io_shard_polled_within(index, b),
                    None => true,
                }
            } else {
                handle.io_shard_polled_within(index, self.all.fresh)
                    && !handle.io_shard_saturated(index)
            };
            if skip {
                continue;
            }
            if let Some(s) = self.poll(handle, index, false) {
                polls += 1;
                saturated += usize::from(s);
                self.all.shards[index].pass_if_unwatched();
            }
        }
        (polls, saturated)
    }

    /// Zero-timeout poll of shard `index` if its driver is free; `full` takes
    /// the full batch. Returns whether the poll filled its batch. No hand-off
    /// here: after polling another shard the caller calls `pass_if_unwatched`;
    /// after its own it parks on it next, or leaves through `Parker::leave`.
    fn poll(&self, handle: &driver::Handle, index: usize, full: bool) -> Option<bool> {
        let mut driver = self.all.shards[index].driver.try_lock()?;
        if full {
            handle.io_shard_request_full(index);
        }
        driver.poll_io(handle);
        Some(handle.io_shard_saturated(index))
    }

    /// Parks the current thread using a condvar for up to `duration`.
    ///
    /// If `duration` is `None`, parks indefinitely until notified.
    ///
    /// # Panics
    ///
    /// Panics if `duration` is `Some` and the duration is zero.
    fn park_condvar(&self, duration: Option<Duration>) {
        // Otherwise we need to coordinate going to sleep
        let mut m = self.mutex.lock();

        match self
            .state
            .compare_exchange(EMPTY, PARKED_CONDVAR, SeqCst, SeqCst)
        {
            Ok(_) => {}
            Err(NOTIFIED) => {
                // We must read here, even though we know it will be `NOTIFIED`.
                // This is because `unpark` may have been called again since we read
                // `NOTIFIED` in the `compare_exchange` above. We must perform an
                // acquire operation that synchronizes with that `unpark` to observe
                // any writes it made before the call to unpark. To do that we must
                // read from the write it made to `state`.
                let old = self.state.swap(EMPTY, SeqCst);
                debug_assert_eq!(old, NOTIFIED, "park state changed unexpectedly");

                return;
            }
            Err(actual) => panic!("inconsistent park state; actual = {actual}"),
        }

        let timeout_at = duration.map(|d| {
            Instant::now()
                .checked_add(d)
                // best effort to avoid overflow and still provide a usable timeout
                .unwrap_or(Instant::now() + Duration::from_secs(1))
        });

        loop {
            let is_timeout;
            (m, is_timeout) = match timeout_at {
                Some(timeout_at) => {
                    let dur = timeout_at.saturating_duration_since(Instant::now());
                    if !dur.is_zero() {
                        // Ideally, we would use `condvar.wait_timeout_until` here, but it is not available
                        // in `loom`. So we manually compute the timeout.
                        let (m, res) = self.condvar.wait_timeout(m, dur).unwrap();
                        (m, res.timed_out())
                    } else {
                        (m, true)
                    }
                }
                None => (self.condvar.wait(m).unwrap(), false),
            };

            if is_timeout {
                match self.state.swap(EMPTY, SeqCst) {
                    PARKED_CONDVAR => return, // timed out, and no notification received
                    NOTIFIED => return,       // notification and timeout happened concurrently
                    actual @ (PARKED_DRIVER | EMPTY) => {
                        panic!("inconsistent park_timeout state, actual = {actual}")
                    }
                    invalid => panic!("invalid park_timeout state, actual = {invalid}"),
                }
            } else if self
                .state
                .compare_exchange(NOTIFIED, EMPTY, SeqCst, SeqCst)
                .is_ok()
            {
                // got a notification
                return;
            }

            // spurious wakeup, go back to sleep
        }
    }

    fn park_driver(
        &self,
        driver: &mut Driver,
        handle: &driver::Handle,
        duration: Option<Duration>,
    ) -> HadDriver {
        if duration.as_ref().is_some_and(Duration::is_zero) {
            // zero duration doesn't actually park the thread, it just
            // polls the I/O events, timers, etc.
            driver.park_timeout(handle, Duration::ZERO);
            return HadDriver::Yes;
        }

        match self
            .state
            .compare_exchange(EMPTY, PARKED_DRIVER, SeqCst, SeqCst)
        {
            Ok(_) => {}
            Err(NOTIFIED) => {
                // We must read here, even though we know it will be `NOTIFIED`.
                // This is because `unpark` may have been called again since we read
                // `NOTIFIED` in the `compare_exchange` above. We must perform an
                // acquire operation that synchronizes with that `unpark` to observe
                // any writes it made before the call to unpark. To do that we must
                // read from the write it made to `state`.
                let old = self.state.swap(EMPTY, SeqCst);
                debug_assert_eq!(old, NOTIFIED, "park state changed unexpectedly");

                return HadDriver::No;
            }
            Err(actual) => panic!("inconsistent park state; actual = {actual}"),
        }

        struct Blocking<'a>(Option<&'a AtomicBool>);
        impl Drop for Blocking<'_> {
            fn drop(&mut self) {
                if let Some(b) = self.0 {
                    b.store(false, SeqCst);
                }
            }
        }
        let _blocking = Blocking(self.all.sharded().then(|| {
            self.shared.blocking.store(true, SeqCst);
            &self.shared.blocking
        }));
        // Sharded: a blocked thread watches one epoll fd, so cap the sleep;
        // the park loop then sweeps the other shards before blocking again.
        let duration = match (self.all.sharded(), self.all.sweep, duration) {
            (true, Some(sweep), Some(d)) => Some(d.min(sweep)),
            (true, Some(sweep), None) => Some(sweep),
            (_, _, d) => d,
        };
        if let Some(duration) = duration {
            debug_assert_ne!(duration, Duration::ZERO);
            driver.park_timeout(handle, duration);
        } else {
            driver.park(handle);
        }
        drop(_blocking);

        match self.state.swap(EMPTY, SeqCst) {
            NOTIFIED => {}      // got a notification, hurray!
            PARKED_DRIVER => {} // no notification, alas
            n => panic!("inconsistent park_timeout state: {n}"),
        }

        HadDriver::Yes
    }

    fn unpark(&self, driver: &driver::Handle) {
        // To ensure the unparked thread will observe any writes we made before
        // this call, we must perform a release operation that `park` can
        // synchronize with. To do that we must write `NOTIFIED` even if `state`
        // is already `NOTIFIED`. That is why this must be a swap rather than a
        // compare-and-swap that returns if it reads `NOTIFIED` on failure.
        match self.state.swap(NOTIFIED, SeqCst) {
            EMPTY => {}    // no one was waiting
            NOTIFIED => {} // already unparked
            PARKED_CONDVAR => self.unpark_condvar(),
            PARKED_DRIVER => driver.unpark_shard(self.shard),
            actual => panic!("inconsistent state in unpark; actual = {actual}"),
        }
    }

    fn unpark_condvar(&self) {
        // There is a period between when the parked thread sets `state` to
        // `PARKED` (or last checked `state` in the case of a spurious wake
        // up) and when it actually waits on `cvar`. If we were to notify
        // during this period it would be ignored and then when the parked
        // thread went to sleep it would never wake up. Fortunately, it has
        // `lock` locked at this stage so we can acquire `lock` to wait until
        // it is ready to receive the notification.
        //
        // Releasing `lock` before the call to `notify_one` means that when the
        // parked thread wakes it doesn't get woken only to have to wait for us
        // to release `lock`.
        drop(self.mutex.lock());

        self.condvar.notify_one();
    }

    fn shutdown(&self, handle: &driver::Handle) {
        if let Some(mut driver) = self.shared.driver.try_lock() {
            driver.shutdown(handle);
        }

        self.condvar.notify_all();
    }
}
