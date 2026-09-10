//! Abstracts out the entire chain of runtime sub-drivers into common types.

// Eventually, this file will see significant refactoring / cleanup. For now, we
// don't need to worry much about dead code with certain feature permutations.
#![cfg_attr(
    any(not(all(tokio_unstable, feature = "full")), target_family = "wasm"),
    allow(dead_code)
)]

use crate::runtime::park::{ParkThread, UnparkThread};

use std::io;
use std::time::Duration;

#[derive(Debug)]
pub(crate) struct Driver {
    inner: TimeDriver,
}

#[derive(Debug)]
pub(crate) struct Handle {
    /// IO driver handle
    pub(crate) io: IoHandle,

    /// Signal driver handle
    #[cfg_attr(any(not(unix), loom), allow(dead_code))]
    pub(crate) signal: SignalHandle,

    /// Time driver handle
    pub(crate) time: TimeHandle,

    /// Source of `Instant::now()`
    #[cfg_attr(not(all(feature = "time", feature = "test-util")), allow(dead_code))]
    pub(crate) clock: Clock,
}

pub(crate) struct Cfg {
    pub(crate) enable_io: bool,
    pub(crate) enable_time: bool,
    pub(crate) enable_pause_time: bool,
    pub(crate) start_paused: bool,
    pub(crate) nevents: usize,
    pub(crate) nevents_busy: Option<usize>,
    pub(crate) timer_flavor: crate::runtime::TimerFlavor,
    /// Number of I/O driver shards (`epoll` instances). 1 = the single-driver behavior.
    pub(crate) io_shards: usize,
}

impl Driver {
    pub(crate) fn new(cfg: Cfg) -> io::Result<(Self, Handle)> {
        let (mut drivers, handle) = Self::new_sharded(Cfg {
            io_shards: 1,
            ..cfg
        })?;
        Ok((drivers.pop().unwrap(), handle))
    }

    /// Creates `cfg.io_shards` drivers sharing one handle. `drivers[i]` polls
    /// I/O shard `i`; shard 0 additionally carries the signal/process drivers.
    /// All of them share the single timer wheel.
    pub(crate) fn new_sharded(cfg: Cfg) -> io::Result<(Vec<Self>, Handle)> {
        let (io_stacks, io_handle, signal_handle) =
            create_io_stacks(cfg.enable_io, cfg.nevents, cfg.nevents_busy, cfg.io_shards)?;

        let clock = create_clock(cfg.enable_pause_time, cfg.start_paused);

        let mut stacks = io_stacks.into_iter();
        let first = stacks.next().expect("at least one io stack");
        let (time_driver, time_handle) =
            create_time_driver(cfg.enable_time, cfg.timer_flavor, first, &clock);

        let mut drivers = vec![Self { inner: time_driver }];
        for io_stack in stacks {
            drivers.push(Self {
                inner: create_secondary_time_driver(cfg.enable_time, cfg.timer_flavor, io_stack),
            });
        }

        Ok((
            drivers,
            Handle {
                io: io_handle,
                signal: signal_handle,
                time: time_handle,
                clock,
            },
        ))
    }

    pub(crate) fn park(&mut self, handle: &Handle) {
        self.inner.park(handle);
    }

    pub(crate) fn park_timeout(&mut self, handle: &Handle, duration: Duration) {
        self.inner.park_timeout(handle, duration);
    }

    /// Zero-timeout poll of the I/O stack only; timers are left to the
    /// driver's own parks. It takes the busy batch unless
    /// `Handle::io_shard_request_full` was called first.
    pub(crate) fn poll_io(&mut self, handle: &Handle) {
        self.inner.poll_io(handle);
    }

    pub(crate) fn shutdown(&mut self, handle: &Handle) {
        self.inner.shutdown(handle);
    }
}

impl Handle {
    pub(crate) fn unpark(&self) {
        #[cfg(feature = "time")]
        if let Some(handle) = &self.time {
            handle.unpark();
        }

        self.io.unpark();
    }

    /// Unparks whoever is parked on I/O shard `shard` only.
    pub(crate) fn unpark_shard(&self, shard: usize) {
        #[cfg(feature = "time")]
        if let Some(handle) = &self.time {
            handle.unpark();
        }

        self.unpark_io_shard(shard);
    }

    cfg_io_driver! {
        fn unpark_io_shard(&self, shard: usize) {
            self.io.unpark_shard(shard);
        }

        /// Whether I/O shard `shard`'s last poll filled its event buffer.
        pub(crate) fn io_shard_saturated(&self, shard: usize) -> bool {
            self.io.as_ref().is_some_and(|io| io.shard_saturated(shard))
        }

        /// Makes the next zero-wait poll of I/O shard `shard` take the full
        /// batch (`max_io_events_per_tick`) instead of the busy batch.
        pub(crate) fn io_shard_request_full(&self, shard: usize) {
            if let Some(io) = self.io.as_ref() {
                io.shard_request_full(shard);
            }
        }

        /// Whether I/O shard `shard` was polled within `within`.
        pub(crate) fn io_shard_polled_within(&self, shard: usize, within: Duration) -> bool {
            self.io
                .as_ref()
                .is_some_and(|io| io.shard_polled_within(shard, within))
        }
    }

    cfg_not_io_driver! {
        fn unpark_io_shard(&self, _shard: usize) {
            self.io.unpark();
        }

        pub(crate) fn io_shard_polled_within(&self, _shard: usize, _within: Duration) -> bool {
            false
        }

        pub(crate) fn io_shard_saturated(&self, _shard: usize) -> bool {
            false
        }

        pub(crate) fn io_shard_request_full(&self, _shard: usize) {}
    }

    cfg_io_driver! {
        #[track_caller]
        pub(crate) fn io(&self) -> &crate::runtime::io::Handle {
            self.io
                .as_ref()
                .expect("A Tokio 1.x context was found, but IO is disabled. Call `enable_io` on the runtime builder to enable IO.")
        }
    }

    cfg_signal_internal_and_unix! {
        #[track_caller]
        pub(crate) fn signal(&self) -> &crate::runtime::signal::Handle {
            self.signal
                .as_ref()
                .expect("A Tokio 1.x context was found, but IO is disabled. Call `enable_io` on the runtime builder to enable IO.")
        }
    }

    cfg_time! {
        /// Returns a reference to the time driver handle.
        ///
        /// Panics if no time driver is present.
        #[track_caller]
        pub(crate) fn time(&self) -> &crate::runtime::time::Handle {
            self.time
                .as_ref()
                .expect("A Tokio 1.x context was found, but timers are disabled. Call `enable_time` on the runtime builder to enable timers.")
        }

        #[cfg(tokio_unstable)]
        pub(crate) fn with_time<F, R>(&self, f: F) -> R
        where
            F: FnOnce(Option<&crate::runtime::time::Handle>) -> R,
        {
            f(self.time.as_ref())
        }

        pub(crate) fn clock(&self) -> &Clock {
            &self.clock
        }
    }
}

// ===== io driver =====

cfg_io_driver! {
    pub(crate) type IoDriver = crate::runtime::io::Driver;

    #[derive(Debug)]
    pub(crate) enum IoStack {
        Enabled(ProcessDriver),
        /// A secondary I/O shard: `epoll` only, no signal/process handling.
        Shard(IoDriver),
        Disabled(ParkThread),
    }

    #[derive(Debug)]
    pub(crate) enum IoHandle {
        Enabled(crate::runtime::io::Handle),
        Disabled(UnparkThread),
    }

    fn create_io_stacks(enabled: bool, nevents: usize, nevents_busy: Option<usize>, shards: usize) -> io::Result<(Vec<IoStack>, IoHandle, SignalHandle)> {
        #[cfg(loom)]
        assert!(!enabled);

        let ret = if enabled {
            let (io_drivers, io_handle) = crate::runtime::io::Driver::new_sharded(nevents, nevents_busy, shards)?;
            let mut io_drivers = io_drivers.into_iter();
            let first = io_drivers.next().expect("at least one shard");

            let (signal_driver, signal_handle) = create_signal_driver(first, &io_handle)?;
            let process_driver = create_process_driver(signal_driver);

            let mut stacks = vec![IoStack::Enabled(process_driver)];
            stacks.extend(io_drivers.map(IoStack::Shard));

            (stacks, IoHandle::Enabled(io_handle), signal_handle)
        } else {
            let park_thread = ParkThread::new();
            let unpark_thread = park_thread.unpark();
            (vec![IoStack::Disabled(park_thread)], IoHandle::Disabled(unpark_thread), Default::default())
        };

        Ok(ret)
    }

    impl IoStack {
        pub(crate) fn park(&mut self, handle: &Handle) {
            match self {
                IoStack::Enabled(v) => v.park(handle),
                IoStack::Shard(v) => v.park(handle),
                IoStack::Disabled(v) => v.park(),
            }
        }

        pub(crate) fn park_timeout(&mut self, handle: &Handle, duration: Duration) {
            match self {
                IoStack::Enabled(v) => v.park_timeout(handle, duration),
                IoStack::Shard(v) => v.park_timeout(handle, duration),
                IoStack::Disabled(v) => v.park_timeout(duration),
            }
        }

        pub(crate) fn poll_io(&mut self, handle: &Handle) {
            self.park_timeout(handle, Duration::ZERO);
        }

        pub(crate) fn shutdown(&mut self, handle: &Handle) {
            match self {
                IoStack::Enabled(v) => v.shutdown(handle),
                IoStack::Shard(v) => v.shutdown(handle),
                IoStack::Disabled(v) => v.shutdown(),
            }
        }
    }

    impl IoHandle {
        pub(crate) fn unpark(&self) {
            match self {
                IoHandle::Enabled(handle) => handle.unpark(),
                IoHandle::Disabled(handle) => handle.unpark(),
            }
        }

        pub(crate) fn unpark_shard(&self, shard: usize) {
            match self {
                IoHandle::Enabled(handle) => handle.unpark_shard(shard),
                IoHandle::Disabled(handle) => handle.unpark(),
            }
        }

        pub(crate) fn as_ref(&self) -> Option<&crate::runtime::io::Handle> {
            match self {
                IoHandle::Enabled(v) => Some(v),
                IoHandle::Disabled(..) => None,
            }
        }
    }
}

cfg_not_io_driver! {
    pub(crate) type IoHandle = UnparkThread;

    #[derive(Debug)]
    pub(crate) struct IoStack(ParkThread);

    fn create_io_stacks(_enabled: bool, _nevents: usize, _nevents_busy: Option<usize>, _shards: usize) -> io::Result<(Vec<IoStack>, IoHandle, SignalHandle)> {
        let park_thread = ParkThread::new();
        let unpark_thread = park_thread.unpark();
        Ok((vec![IoStack(park_thread)], unpark_thread, Default::default()))
    }

    impl IoStack {
        pub(crate) fn park(&mut self, _handle: &Handle) {
            self.0.park();
        }

        pub(crate) fn park_timeout(&mut self, _handle: &Handle, duration: Duration) {
            self.0.park_timeout(duration);
        }

        pub(crate) fn poll_io(&mut self, _handle: &Handle) {
            self.0.park_timeout(Duration::ZERO);
        }

        pub(crate) fn shutdown(&mut self, _handle: &Handle) {
            self.0.shutdown();
        }

        /// This is not a "real" driver, so it is not considered enabled.
        pub(crate) fn is_enabled(&self) -> bool {
            false
        }
    }
}

// ===== signal driver =====

cfg_signal_internal_and_unix! {
    type SignalDriver = crate::runtime::signal::Driver;
    pub(crate) type SignalHandle = Option<crate::runtime::signal::Handle>;

    fn create_signal_driver(io_driver: IoDriver, io_handle: &crate::runtime::io::Handle) -> io::Result<(SignalDriver, SignalHandle)> {
        let driver = crate::runtime::signal::Driver::new(io_driver, io_handle)?;
        let handle = driver.handle();
        Ok((driver, Some(handle)))
    }
}

cfg_not_signal_internal_and_unix! {
    pub(crate) type SignalHandle = ();

    cfg_io_driver! {
        type SignalDriver = IoDriver;

        fn create_signal_driver(io_driver: IoDriver, _io_handle: &crate::runtime::io::Handle) -> io::Result<(SignalDriver, SignalHandle)> {
            Ok((io_driver, ()))
        }
    }
}

// ===== process driver =====

cfg_process_driver! {
    type ProcessDriver = crate::runtime::process::Driver;

    fn create_process_driver(signal_driver: SignalDriver) -> ProcessDriver {
        ProcessDriver::new(signal_driver)
    }
}

cfg_not_process_driver! {
    cfg_io_driver! {
        type ProcessDriver = SignalDriver;

        fn create_process_driver(signal_driver: SignalDriver) -> ProcessDriver {
            signal_driver
        }
    }
}

// ===== time driver =====

cfg_time! {
    #[derive(Debug)]
    pub(crate) enum TimeDriver {
        Enabled {
            driver: crate::runtime::time::Driver,
        },
        EnabledAlt(IoStack),
        Disabled(IoStack),
    }

    pub(crate) type Clock = crate::time::Clock;
    pub(crate) type TimeHandle = Option<crate::runtime::time::Handle>;

    fn create_clock(enable_pausing: bool, start_paused: bool) -> Clock {
        crate::time::Clock::new(enable_pausing, start_paused)
    }

    fn create_time_driver(
        enable: bool,
        timer_flavor: crate::runtime::TimerFlavor,
        io_stack: IoStack,
        clock: &Clock,
    ) -> (TimeDriver, TimeHandle) {
        if enable {
            match timer_flavor {
                crate::runtime::TimerFlavor::Traditional => {
                    let (driver, handle) = crate::runtime::time::Driver::new(io_stack, clock);
                    (TimeDriver::Enabled { driver }, Some(handle))
                }
                #[cfg(all(tokio_unstable, feature = "rt-multi-thread"))]
                crate::runtime::TimerFlavor::Alternative => {
                    (TimeDriver::EnabledAlt(io_stack), Some(crate::runtime::time::Driver::new_alt(clock)))
                }
            }
        } else {
            (TimeDriver::Disabled(io_stack), None)
        }
    }

    /// Time driver wrapper for a secondary I/O shard: shares the wheel created
    /// by `create_time_driver` (via the runtime `Handle`), owns only its park.
    fn create_secondary_time_driver(
        enable: bool,
        timer_flavor: crate::runtime::TimerFlavor,
        io_stack: IoStack,
    ) -> TimeDriver {
        if enable {
            match timer_flavor {
                crate::runtime::TimerFlavor::Traditional => TimeDriver::Enabled {
                    driver: crate::runtime::time::Driver::from_park(io_stack),
                },
                #[cfg(all(tokio_unstable, feature = "rt-multi-thread"))]
                crate::runtime::TimerFlavor::Alternative => TimeDriver::EnabledAlt(io_stack),
            }
        } else {
            TimeDriver::Disabled(io_stack)
        }
    }

    impl TimeDriver {
        pub(crate) fn park(&mut self, handle: &Handle) {
            match self {
                TimeDriver::Enabled { driver, .. } => driver.park(handle),
                TimeDriver::EnabledAlt(v) => v.park(handle),
                TimeDriver::Disabled(v) => v.park(handle),
            }
        }

        pub(crate) fn park_timeout(&mut self, handle: &Handle, duration: Duration) {
            match self {
                TimeDriver::Enabled { driver } => driver.park_timeout(handle, duration),
                TimeDriver::EnabledAlt(v) => v.park_timeout(handle, duration),
                TimeDriver::Disabled(v) => v.park_timeout(handle, duration),
            }
        }

        pub(crate) fn poll_io(&mut self, handle: &Handle) {
            match self {
                TimeDriver::Enabled { driver } => driver.poll_io(handle),
                TimeDriver::EnabledAlt(v) | TimeDriver::Disabled(v) => v.poll_io(handle),
            }
        }

        pub(crate) fn shutdown(&mut self, handle: &Handle) {
            match self {
                TimeDriver::Enabled { driver } => driver.shutdown(handle),
                TimeDriver::EnabledAlt(v) => v.shutdown(handle),
                TimeDriver::Disabled(v) => v.shutdown(handle),
            }
        }
    }
}

cfg_not_time! {
    type TimeDriver = IoStack;

    pub(crate) type Clock = ();
    pub(crate) type TimeHandle = ();

    fn create_clock(_enable_pausing: bool, _start_paused: bool) -> Clock {
        ()
    }

    fn create_time_driver(
        _enable: bool,
        _timer_flavor: crate::runtime::TimerFlavor,
        io_stack: IoStack,
        _clock: &Clock,
    ) -> (TimeDriver, TimeHandle) {
        (io_stack, ())
    }

    fn create_secondary_time_driver(
        _enable: bool,
        _timer_flavor: crate::runtime::TimerFlavor,
        io_stack: IoStack,
    ) -> TimeDriver {
        io_stack
    }
}

cfg_io_uring! {
    pub(crate) mod op;
}
