//! A mocked clock for use with `tokio_timer` based futures.
//!
//! # Example
//!
//! ```
//! use tokio_test::clock;
//! use tokio_test::{assert_ready, assert_not_ready};
//! use tokio_timer::Delay;
//! use std::time::Duration;
//! use futures::Future;
//!
//! clock::mock(|handle| {
//!     let mut delay = Delay::new(handle.now() + Duration::from_secs(1));
//!
//!     assert_not_ready!(delay.poll());
//!
//!     handle.advance(Duration::from_secs(1));
//!
//!     assert_ready!(delay.poll());
//! });
//! ```

use tokio_executor::park::{Park, Unpark};
use tokio_timer::clock::{Clock, Now};
use tokio_timer::Timer;

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Run the provided closure with a `MockClock` that starts at the current time.
pub fn mock<F, R>(f: F) -> R
where
    F: FnOnce(&mut Handle) -> R,
{
    let mut mock = MockClock::new();
    mock.enter(f)
}

/// Run the provided closure with a `MockClock` that starts at the provided `Instant`.
pub fn mock_at<F, R>(instant: Instant, f: F) -> R
where
    F: FnOnce(&mut Handle) -> R,
{
    let mut mock = MockClock::with_instant(instant);
    mock.enter(f)
}

/// Mock clock for use with `tokio-timer` futures.
///
/// A mock timer that is able to advance and wake after a
/// certain duration.
#[derive(Debug)]
pub struct MockClock {
    time: MockTime,
    clock: Clock,
}

/// A handle to the `MockClock`.
#[derive(Debug)]
pub struct Handle {
    timer: Timer<MockPark>,
    time: MockTime,
}

type Inner = Arc<Mutex<State>>;

#[derive(Debug, Clone)]
struct MockTime {
    inner: Inner,
    _pd: PhantomData<Rc<()>>,
}

#[derive(Debug)]
struct MockNow {
    inner: Inner,
}

#[derive(Debug)]
struct MockPark {
    inner: Inner,
    _pd: PhantomData<Rc<()>>,
}

#[derive(Debug)]
struct MockUnpark {
    inner: Inner,
}

#[derive(Debug)]
struct State {
    base: Instant,
    advance: Duration,
    unparked: bool,
    park_for: Option<Duration>,
}

impl MockClock {
    /// Create a new `MockClock` with the current time.
    pub fn new() -> Self {
        MockClock::with_instant(Instant::now())
    }

    /// Create a `MockClock` with its current time at a duration from now
    ///
    /// This will create a clock with `Instant::now() + duration` as the current time.
    pub fn with_duration(duration: Duration) -> Self {
        let instant = Instant::now() + duration;
        MockClock::with_instant(instant)
    }

    /// Create a `MockClock` that sets its current time as the `Instant` provided.
    pub fn with_instant(instant: Instant) -> Self {
        let time = MockTime::new(instant);
        let clock = Clock::new_with_now(time.mock_now());

        MockClock { time, clock }
    }

    /// Enter the `MockClock` context.
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Handle) -> R,
    {
        ::tokio_timer::clock::with_default(&self.clock, || {
            let park = self.time.mock_park();
            let timer = Timer::new(park);
            let handle = timer.handle();
            let time = self.time.clone();

            ::tokio_timer::with_default(&handle, || {
                let mut handle = Handle::new(timer, time);
                f(&mut handle)
                // lazy(|| Ok::<_, ()>(f(&mut handle))).wait().unwrap()
            })
        })
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Handle {
    pub(self) fn new(timer: Timer<MockPark>, time: MockTime) -> Self {
        Handle { timer, time }
    }

    /// Turn the internal timer and mock park for the provided duration.
    pub fn turn(&mut self) {
        self.timer.turn(None).unwrap();
    }

    /// Turn the internal timer and mock park for the provided duration.
    pub fn turn_for(&mut self, duration: Duration) {
        self.timer.turn(Some(duration)).unwrap();
    }

    /// Advance the `MockClock` by the provided duration.
    pub fn advance(&mut self, duration: Duration) {
        let inner = self.timer.get_park().inner.clone();
        let deadline = inner.lock().unwrap().now() + duration;

        while inner.lock().unwrap().now() < deadline {
            let dur = deadline - inner.lock().unwrap().now();
            self.turn_for(dur);
        }
    }

    /// Returns the total amount of time the time has been advanced.
    pub fn advanced(&self) -> Duration {
        self.time.inner.lock().unwrap().advance
    }

    /// Get the currently mocked time
    pub fn now(&mut self) -> Instant {
        self.time.now()
    }

    /// Turn the internal timer once, but force "parking" for `duration` regardless of any pending
    /// timeouts
    pub fn park_for(&mut self, duration: Duration) {
        self.time.inner.lock().unwrap().park_for = Some(duration);
        self.turn()
    }
}

impl MockTime {
    pub(crate) fn new(now: Instant) -> MockTime {
        let state = State {
            base: now,
            advance: Duration::default(),
            unparked: false,
            park_for: None,
        };

        MockTime {
            inner: Arc::new(Mutex::new(state)),
            _pd: PhantomData,
        }
    }

    pub(crate) fn mock_now(&self) -> MockNow {
        let inner = self.inner.clone();
        MockNow { inner }
    }

    pub(crate) fn mock_park(&self) -> MockPark {
        let inner = self.inner.clone();
        MockPark {
            inner,
            _pd: PhantomData,
        }
    }

    pub(crate) fn now(&self) -> Instant {
        self.inner.lock().unwrap().now()
    }
}

impl State {
    fn now(&self) -> Instant {
        self.base + self.advance
    }

    fn advance(&mut self, duration: Duration) {
        self.advance += duration;
    }
}

impl Park for MockPark {
    type Unpark = MockUnpark;
    type Error = ();

    fn unpark(&self) -> Self::Unpark {
        let inner = self.inner.clone();
        MockUnpark { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        let mut inner = self.inner.lock().map_err(|_| ())?;

        let duration = inner.park_for.take().expect("call park_for first");

        inner.advance(duration);
        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(duration) = inner.park_for.take() {
            inner.advance(duration);
        } else {
            inner.advance(duration);
        }

        Ok(())
    }
}

impl Unpark for MockUnpark {
    fn unpark(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.unparked = true;
        }
    }
}

impl Now for MockNow {
    fn now(&self) -> Instant {
        self.inner.lock().unwrap().now()
    }
}
