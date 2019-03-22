use executor::current_thread::CurrentThread;
use runtime::current_thread::Runtime;

use tokio_reactor::Reactor;
use tokio_timer::clock::Clock;
use tokio_timer::timer::Timer;

use std::io;

/// Builds a Single-threaded runtime with custom configuration values.
///
/// Methods can be chained in order to set the configuration values. The
/// Runtime is constructed by calling [`build`].
///
/// New instances of `Builder` are obtained via [`Builder::new`].
///
/// See function level documentation for details on the various configuration
/// settings.
///
/// [`build`]: #method.build
/// [`Builder::new`]: #method.new
///
/// # Examples
///
/// ```
/// extern crate tokio;
/// extern crate tokio_timer;
///
/// use tokio::runtime::current_thread::Builder;
/// use tokio_timer::clock::Clock;
///
/// # pub fn main() {
/// // build Runtime
/// let runtime = Builder::new()
///     .clock(Clock::new())
///     .build();
/// // ... call runtime.run(...)
/// # let _ = runtime;
/// # }
/// ```
#[derive(Debug)]
pub struct Builder {
    /// The clock to use
    clock: Clock,
}

impl Builder {
    /// Returns a new runtime builder initialized with default configuration
    /// values.
    ///
    /// Configuration methods can be chained on the return value.
    pub fn new() -> Builder {
        Builder {
            clock: Clock::new(),
        }
    }

    /// Set the `Clock` instance that will be used by the runtime.
    pub fn clock(&mut self, clock: Clock) -> &mut Self {
        self.clock = clock;
        self
    }

    /// Create the configured `Runtime`.
    pub fn build(&mut self) -> io::Result<Runtime> {
        // We need a reactor to receive events about IO objects from kernel
        let reactor = Reactor::new()?;
        let reactor_handle = reactor.handle();

        // Place a timer wheel on top of the reactor. If there are no timeouts to fire, it'll let the
        // reactor pick up some new external events.
        let timer = Timer::new_with_now(reactor, self.clock.clone());
        let timer_handle = timer.handle();

        // And now put a single-threaded executor on top of the timer. When there are no futures ready
        // to do something, it'll let the timer or the reactor to generate some new stimuli for the
        // futures to continue in their life.
        let executor = CurrentThread::new_with_park(timer);

        let runtime = Runtime::new2(
            reactor_handle,
            timer_handle,
            self.clock.clone(),
            executor);

        Ok(runtime)
    }
}
