//! A `current_thread` runtime whose wait belongs to a host event loop.
//!
//! The scheduler drives to a fixed point, then waits. A native runtime waits
//! by parking its thread in the driver; an event loop leaves the wait to the
//! host. It exposes what the host needs to wait on its behalf, a descriptor
//! that becomes readable when the loop has work, a callback for the same, and
//! the time until the soonest timer, and the host calls [`EventLoop::tick`]
//! when either fires, for one zero-timeout turn of the driver and one batch
//! of tasks. A host that would rather block can do so in
//! [`EventLoop::park`], which waits in the driver without running tasks.
//!
//! Everything that would unpark a native runtime's thread notifies the host:
//! I/O readiness and signals through the reactor's descriptor, and spawns,
//! wakes from other threads, nearer timers and [`Unparker::unpark`] through
//! the driver's waker, which is registered in the same set, and through the
//! [`on_work_available`] callback. Nothing is polled and no thread is owned.
//!
//! On `wasm32-unknown-emscripten` without threads the JavaScript host loop
//! can be the host itself: a *hosted* event loop (`emscripten.rs`) registers
//! the descriptor and the timeout with it, so the program spawns and returns
//! to the host.
//!
//! [`on_work_available`]: crate::runtime::Builder::on_work_available

#[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
pub(crate) mod emscripten;
mod signal;
pub(crate) use signal::{Signal, SignalCfg};

use crate::runtime::local_runtime::LocalRuntime;
use crate::runtime::scheduler::current_thread::CurrentThread;
use crate::runtime::{context, EnterGuard, Handle, Runtime};
use crate::task::JoinHandle;
use crate::util::trace::SpawnMeta;

use std::cell::{Ref, RefCell, RefMut};
use std::future::Future;
#[cfg(unix)]
use std::os::fd::BorrowedFd;
use std::rc::Rc;
use std::time::Duration;

/// A runtime driven by a host event loop instead of by parking a thread.
///
/// Built with [`Builder::build_event_loop`] from a `current_thread` builder.
/// The host waits on [`event_fd`](Self::event_fd) (or for
/// [`on_work_available`]) with [`next_sleep`](Self::next_sleep) as its
/// timeout, and on either calls [`tick`](Self::tick) until `next_sleep` is
/// no longer `Some(ZERO)`:
///
/// ```ignore
/// loop {
///     el.tick(64);
///     if el.next_sleep() != Some(Duration::ZERO) {
///         break;
///     }
/// }
/// ```
///
/// The builder's `event_interval` has no role: the host's `max_tasks` bounds
/// each batch.
///
/// Tasks are submitted through the [`Handle`] from any thread, and run in
/// batches from those ticks. [`Handle::block_on`] from another thread works
/// as on a native runtime. The event loop is `Send`: like a [`Runtime`] it
/// may be ticked from any thread, one at a time. [`LocalEventLoop`] is the
/// `!Send` counterpart with [`spawn_local`](LocalEventLoop::spawn_local).
///
/// Dropping the `EventLoop` shuts the runtime down as dropping a `Runtime`
/// does. The descriptor is closed with it, so the host must remove it from
/// its set first.
///
/// # Examples
///
/// For a host that waits in `poll(2)`:
///
/// ```no_run
/// # #[cfg(unix)]
/// # fn main() -> std::io::Result<()> {
/// use std::os::fd::AsRawFd;
/// use std::time::Duration;
/// use tokio::runtime::Builder;
///
/// let mut el = Builder::new_current_thread()
///     .enable_all()
///     .build_event_loop()?;
///
/// let done = el.handle().spawn(async {
///     tokio::time::sleep(Duration::from_millis(10)).await;
/// });
///
/// while !done.is_finished() {
///     let fd = el.event_fd().expect("the I/O driver is enabled");
///     let mut pfd = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
///     let timeout = el.next_sleep().map_or(-1, |t| t.as_millis() as i32);
///     unsafe { libc::poll(&mut pfd, 1, timeout) };
///     loop {
///         el.tick(64);
///         if el.next_sleep() != Some(Duration::ZERO) {
///             break;
///         }
///     }
/// }
/// # Ok(()) }
/// # #[cfg(not(unix))]
/// # fn main() {}
/// ```
///
/// For a host with no descriptor-based wait, on any platform, the event loop
/// posts to the host's own queue through [`on_work_available`], and the
/// host sleeps on that queue with `next_sleep` as the timeout. This suffices
/// without the I/O driver, as here; with it, I/O readiness reaches neither
/// the callback nor the queue, so where `event_fd` is `None` the host waits
/// in `park` instead.
///
/// ```
/// use std::sync::mpsc;
/// use std::time::Duration;
/// use tokio::runtime::Builder;
///
/// # fn main() -> std::io::Result<()> {
/// let (tx, host_queue) = mpsc::channel();
/// let mut el = Builder::new_current_thread()
///     .enable_time()
///     .on_work_available(move || { let _ = tx.send(()); })
///     .build_event_loop()?;
///
/// let done = el.handle().spawn(async {
///     tokio::time::sleep(Duration::from_millis(10)).await;
/// });
///
/// while !done.is_finished() {
///     match el.next_sleep() {
///         Some(Duration::ZERO) => {}
///         Some(timeout) => { let _ = host_queue.recv_timeout(timeout); }
///         None => { let _ = host_queue.recv(); }
///     }
///     el.tick(64);
/// }
/// # Ok(()) }
/// ```
///
/// For a host that gives tokio the thread's wait, [`park`](Self::park)
/// blocks in the driver for I/O, a timer, or a wakeup, and is the way to
/// wait for I/O where `event_fd` is `None`. An [`Unparker`] interrupts it.
///
/// ```
/// use tokio::runtime::Builder;
///
/// # fn main() -> std::io::Result<()> {
/// let mut el = Builder::new_current_thread()
///     .enable_all()
///     .build_event_loop()?;
///
/// let done = el.handle().spawn(async { 1 + 1 });
///
/// while !done.is_finished() {
///     el.park(None);
///     el.tick(64);
/// }
/// # Ok(()) }
/// ```
///
/// [`Builder::build_event_loop`]: crate::runtime::Builder::build_event_loop
/// [`on_work_available`]: crate::runtime::Builder::on_work_available
/// [`Handle::block_on`]: crate::runtime::Handle::block_on
#[derive(Debug)]
pub struct EventLoop {
    core: Core<Runtime>,
}

/// A [`LocalRuntime`] driven by a host event loop instead of by parking a
/// thread.
///
/// Built with [`Builder::build_local_event_loop`]. The same as
/// [`EventLoop`], with [`spawn_local`](Self::spawn_local) for `!Send`
/// futures; like `LocalRuntime` it is `!Send`, so the host drives it on the
/// thread that built it.
///
/// On `wasm32-unknown-emscripten` without threads, a *hosted* event loop
/// (`Builder::build_hosted_local_event_loop`) is driven by the JavaScript
/// host loop itself: the program spawns and returns to the host. Such a loop
/// keeps the Emscripten runtime alive while it has tasks. A
/// [`Handle::block_on`] suspended through JSPI on the same thread defers
/// hosted ticks until it returns. One of its own tasks may drop it or shut
/// it down: the loop then shuts down as the host's tick returns, cancelling
/// its other tasks.
///
/// [`Builder::build_local_event_loop`]: crate::runtime::Builder::build_local_event_loop
/// [`Handle::block_on`]: crate::runtime::Handle::block_on
#[derive(Debug)]
pub struct LocalEventLoop {
    handle: Handle,
    /// Detaches from the host before the runtime, and its descriptor, drop.
    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
    hosted: Option<Rc<emscripten::Hosted>>,
    /// Shared weakly with the host's callbacks on a hosted loop, which tick
    /// between the program's own calls; never while one is in progress, as
    /// a tick does not yield to the host. A callback holds it for its tick,
    /// so a task dropping the loop releases it only once that tick returns.
    core: Rc<RefCell<Core<LocalRuntime>>>,
}

/// What both event loops share.
#[derive(Debug)]
struct Core<R> {
    runtime: R,
    /// Whether the last tick or park left tasks in the local queue. The
    /// local queue only changes while the loop is ticked or parked.
    local_ready: bool,
}

/// `Runtime` or `LocalRuntime`, built `current_thread`.
trait Scheduler {
    fn handle(&self) -> &Handle;
    fn current_thread(&self) -> &CurrentThread;
}

impl Scheduler for Runtime {
    fn handle(&self) -> &Handle {
        Runtime::handle(self)
    }
    fn current_thread(&self) -> &CurrentThread {
        Runtime::current_thread(self)
    }
}

impl Scheduler for LocalRuntime {
    fn handle(&self) -> &Handle {
        LocalRuntime::handle(self)
    }
    fn current_thread(&self) -> &CurrentThread {
        LocalRuntime::current_thread(self)
    }
}

impl<R: Scheduler> Core<R> {
    fn new(runtime: R) -> Core<R> {
        Core {
            runtime,
            local_ready: false,
        }
    }

    fn handle(&self) -> &Handle {
        self.runtime.handle()
    }

    fn tick(&mut self, max_tasks: usize) -> usize {
        let handle = self.runtime.handle();
        let tick = || {
            context::enter_runtime(&handle.inner, false, |_| {
                self.runtime
                    .current_thread()
                    .tick(handle.inner.as_current_thread(), max_tasks)
            })
        };
        // The driver's turn must neither yield to nor suspend on the host
        // loop: a host callback already has the turn.
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        let (ran, local_ready) = crate::runtime::jspi::host_turn(tick);
        #[cfg(not(all(target_os = "emscripten", not(target_feature = "atomics"))))]
        let (ran, local_ready) = tick();
        self.local_ready = local_ready;
        ran
    }

    fn park(&mut self, timeout: Option<Duration>) {
        let handle = self.runtime.handle();
        self.local_ready = context::enter_runtime(&handle.inner, false, |_| {
            self.runtime
                .current_thread()
                .park(handle.inner.as_current_thread(), timeout)
        });
    }

    fn next_sleep(&self) -> Option<Duration> {
        match self.next_wake() {
            NextWake::Ready => Some(Duration::ZERO),
            NextWake::Timer(tick) => Some(self.until_tick(tick)),
            NextWake::Never => None,
        }
    }

    /// [`next_sleep`](Self::next_sleep) without the clock read: a timer
    /// deadline as the wheel's tick, which a host compares against the one
    /// it armed.
    fn next_wake(&self) -> NextWake {
        let handle = self.handle();
        let driver = handle.inner.driver();
        if self.local_ready
            || handle.inner.as_current_thread().has_injected_work()
            || driver.has_pending_wakeup()
        {
            return NextWake::Ready;
        }
        #[cfg(feature = "time")]
        if let Some(tick) = driver.time.as_ref().and_then(|time| time.next_expiration()) {
            return NextWake::Timer(tick);
        }
        NextWake::Never
    }

    #[cfg(feature = "time")]
    fn until_tick(&self, tick: u64) -> Duration {
        let driver = self.handle().inner.driver();
        let time = driver
            .time
            .as_ref()
            .expect("a timer deadline needs the time driver");
        time.until_tick(tick, &driver.clock)
    }

    #[cfg(not(feature = "time"))]
    fn until_tick(&self, _tick: u64) -> Duration {
        unreachable!("no timer deadline without the time driver")
    }
}

/// What the host waits for after a turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextWake {
    /// Work is ready now.
    Ready,
    /// Nothing until the timer wheel's tick, unless woken.
    #[cfg_attr(not(feature = "time"), allow(dead_code))]
    Timer(u64),
    /// Nothing scheduled.
    Never,
}

/// Wakes a parked [`EventLoop`] or [`LocalEventLoop`] from any thread.
///
/// Obtained from [`EventLoop::unparker`]. An `unpark` makes the current or
/// next [`park`](EventLoop::park) return, and notifies the host through
/// [`event_fd`](EventLoop::event_fd), [`on_work_available`] and
/// [`next_sleep`](EventLoop::next_sleep), which reports `Some(ZERO)` until
/// a tick or park consumes it. Unlike a wakeup that carries work, it
/// always reaches the callback, during a tick too: that tick consumes it
/// without leaving anything for `next_sleep` to show after.
///
/// [`on_work_available`]: crate::runtime::Builder::on_work_available
#[derive(Clone, Debug)]
pub struct Unparker {
    handle: Handle,
}

impl Unparker {
    /// Wakes the event loop: a current or future `park` returns.
    pub fn unpark(&self) {
        self.handle.inner.driver().unpark_explicit();
    }
}

macro_rules! event_loop_methods {
    () => {
        /// Returns a handle to the runtime, for spawning from any thread.
        pub fn handle(&self) -> &Handle {
            self.handle_ref()
        }

        /// Enters the runtime context, so that `tokio::spawn` and runtime
        /// aware types resolve to this event loop.
        pub fn enter(&self) -> EnterGuard<'_> {
            self.handle_ref().enter()
        }

        /// Polls the I/O driver without blocking, then polls up to
        /// `max_tasks` ready tasks. Returns the number of tasks polled.
        /// `max_tasks == 0` drains the driver without running any tasks.
        ///
        /// Call this on a wakeup or when [`next_sleep`](Self::next_sleep) has
        /// elapsed, and again while `next_sleep` is `Some(ZERO)`. A wakeup
        /// does not guarantee work (a cancelled timer, a cancelled
        /// registration), so a tick that polls nothing is normal, and a tick
        /// without a wakeup is harmless.
        ///
        /// # Panics
        ///
        /// Panics if called from within a runtime, including from one of this
        /// loop's own tasks, or if a task panicked and the runtime is
        /// configured to [shut down on unhandled panics]; the loop is then
        /// unusable, and every further tick panics the same way.
        ///
        /// [shut down on unhandled panics]: crate::runtime::Builder::unhandled_panic
        pub fn tick(&mut self, max_tasks: usize) -> usize {
            let ran = self.core_mut().tick(max_tasks);
            self.after_turn();
            ran
        }

        /// How long the host may sleep before calling [`tick`](Self::tick)
        /// again.
        ///
        /// * `Some(ZERO)`: work is ready now, or an [`Unparker`] woke the
        ///   loop since the last tick or park; tick again.
        /// * `Some(d)`: nothing until a timer fires in `d`, unless woken.
        /// * `None`: nothing scheduled; wait for a wakeup.
        ///
        /// Timers do not notify the host, so this is the timeout of its wait
        /// on [`event_fd`](Self::event_fd). Re-read it after every tick and
        /// wakeup: a tick may register a nearer timer, and a nearer timer
        /// registered from another thread, or by a `Handle::block_on`
        /// elsewhere, wakes the host so that it does. A wakeup that arrives
        /// while the loop is being ticked is reported here and by the
        /// descriptor, but not by [`on_work_available`], so a host on the
        /// callback must tick until this is no longer `Some(ZERO)` before
        /// waiting.
        ///
        /// [`on_work_available`]: crate::runtime::Builder::on_work_available
        pub fn next_sleep(&self) -> Option<Duration> {
            self.core_ref().next_sleep()
        }

        /// A descriptor that becomes readable whenever the loop has work
        /// other than a timer, for the host to add to its own wait set.
        ///
        /// With the I/O driver this is the reactor's own `epoll` or `kqueue`
        /// descriptor, readable while it has pending events: I/O readiness and
        /// signals directly, and spawns, wakes from other threads, nearer
        /// timers and [`Unparker::unpark`] through the driver's waker. It is
        /// level-triggered; a tick consumes the events. Without the I/O
        /// driver it is the standalone descriptor of [`enable_event_fd`], or
        /// `None`. It is also `None` with the I/O driver on a target whose
        /// reactor has no descriptor to expose.
        ///
        /// Timers never make it readable, so sleep on it with
        /// [`next_sleep`](Self::next_sleep) as the timeout.
        ///
        /// [`enable_event_fd`]: crate::runtime::Builder::enable_event_fd
        #[cfg(unix)]
        #[cfg_attr(docsrs, doc(cfg(unix)))]
        pub fn event_fd(&self) -> Option<BorrowedFd<'_>> {
            self.handle_ref().inner.driver().event_fd()
        }

        /// Blocks until the loop has work, a timer expires, `timeout`
        /// elapses, or [`Unparker::unpark`] is called. May return spuriously.
        ///
        /// Waits in the driver for `min(timeout, next_sleep())`, dispatching
        /// I/O readiness, signals and due timers to the tasks that wait on
        /// them, but running none: the next [`tick`](Self::tick) does. A
        /// `timeout` of `None` waits indefinitely for a wakeup or a timer;
        /// with neither pending (no tasks, no I/O driver) only an `Unparker`
        /// can end it.
        ///
        /// This is for a host that owns the thread's wait but has no
        /// descriptor-based wait to add [`event_fd`](Self::event_fd) to, and
        /// where the I/O driver is enabled on a target that exposes no
        /// descriptor, the only way to wait for I/O.
        ///
        /// # Panics
        ///
        /// Panics if called from within a runtime, including from one of this
        /// loop's own tasks.
        pub fn park(&mut self, timeout: Option<Duration>) {
            self.core_mut().park(timeout);
            self.after_turn();
        }

        /// Returns an [`Unparker`] that wakes this loop from any thread.
        pub fn unparker(&self) -> Unparker {
            Unparker {
                handle: self.handle_ref().clone(),
            }
        }

        /// Shuts down the runtime, waiting for at most `duration` for all
        /// spawned work to stop.
        ///
        /// See [`Runtime::shutdown_timeout`].
        ///
        /// [`Runtime::shutdown_timeout`]: crate::runtime::Runtime::shutdown_timeout
        pub fn shutdown_timeout(self, duration: Duration) {
            if let Some(runtime) = self.into_runtime() {
                runtime.shutdown_timeout(duration);
            }
        }

        /// Shuts down the runtime, without waiting for any spawned work to
        /// stop.
        ///
        /// See [`Runtime::shutdown_background`].
        ///
        /// [`Runtime::shutdown_background`]: crate::runtime::Runtime::shutdown_background
        pub fn shutdown_background(self) {
            if let Some(runtime) = self.into_runtime() {
                runtime.shutdown_background();
            }
        }
    };
}

impl EventLoop {
    pub(crate) fn new(runtime: Runtime) -> EventLoop {
        EventLoop {
            core: Core::new(runtime),
        }
    }

    fn handle_ref(&self) -> &Handle {
        self.core.handle()
    }

    fn core_ref(&self) -> &Core<Runtime> {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core<Runtime> {
        &mut self.core
    }

    fn after_turn(&self) {}

    fn into_runtime(self) -> Option<Runtime> {
        Some(self.core.runtime)
    }

    event_loop_methods!();
}

impl LocalEventLoop {
    pub(crate) fn new(runtime: LocalRuntime) -> LocalEventLoop {
        LocalEventLoop {
            handle: runtime.handle().clone(),
            #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
            hosted: None,
            core: Rc::new(RefCell::new(Core::new(runtime))),
        }
    }

    /// Attaches the loop to the ambient JavaScript host loop. `hold` is the
    /// loop's hold on the Emscripten runtime, already wired to the driver's
    /// host signal.
    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
    pub(crate) fn hosted(
        runtime: LocalRuntime,
        hold: std::sync::Arc<emscripten::Hold>,
    ) -> std::io::Result<LocalEventLoop> {
        let mut el = LocalEventLoop::new(runtime);
        el.hosted = Some(emscripten::Hosted::attach(&el.core, hold)?);
        Ok(el)
    }

    fn handle_ref(&self) -> &Handle {
        &self.handle
    }

    fn core_ref(&self) -> Ref<'_, Core<LocalRuntime>> {
        self.core.borrow()
    }

    fn core_mut(&self) -> RefMut<'_, Core<LocalRuntime>> {
        self.core.borrow_mut()
    }

    /// A hosted loop re-arms the host after a turn taken by the program.
    fn after_turn(&self) {
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        if let Some(hosted) = &self.hosted {
            hosted.after_turn();
        }
    }

    /// `None` when a hosted loop is shut down from its own tick: the host's
    /// callback still holds the core, and drops the runtime as the tick
    /// returns. Without threads nothing is left to wait for, so that is the
    /// background shutdown.
    fn into_runtime(self) -> Option<LocalRuntime> {
        #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
        drop(self.hosted);
        let core = Rc::try_unwrap(self.core).ok()?;
        Some(core.into_inner().runtime)
    }

    /// Spawns a future onto the runtime. It is queued, and the host
    /// notified; it never runs before `spawn_local` returns.
    ///
    /// Unlike [`Handle::spawn`], the future may be `!Send`: the event loop is
    /// `!Send`, so it is only ever polled on this thread.
    #[track_caller]
    pub fn spawn_local<F>(&self, future: F) -> JoinHandle<F::Output>
    where
        F: Future + 'static,
        F::Output: 'static,
    {
        let meta = SpawnMeta::new_unnamed(std::mem::size_of::<F>());
        // SAFETY: `LocalEventLoop` is `!Send`, so this is the thread that
        // built the runtime, and `tick` polls only on that thread.
        unsafe { self.handle.spawn_local_named(future, meta) }
    }

    event_loop_methods!();
}
