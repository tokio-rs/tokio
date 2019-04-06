//! Mock timer utilities
//!
//! # Example
//!
//! ```
//! # #[macro_use] extern crate tokio_test;
//! # extern crate futures;
//! # extern crate tokio_timer;
//! # use tokio_test::timer::{mocked, turn};
//! # use tokio_timer::Delay;
//! # use std::time::Duration;
//! # use futures::Future;
//! mocked(|timer, time| {
//!     let mut delay = Delay::new(time.now() + Duration::from_secs(1));
//!
//!     assert_not_ready!(delay.poll());
//!     turn(timer, Duration::from_secs(2));
//!     turn(timer, None);
//!     assert_ready!(delay.poll());
//! });
//! ```

use tokio_executor::park::{Park, Unpark};
use tokio_timer::clock::Now;
use tokio_timer::timer::Timer;

use futures::future::{lazy, Future};

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Mock timber
///
/// A mock timer that is able to advance and wake after a
/// certain duration.
#[derive(Debug)]
pub struct MockTime {
    inner: Inner,
    _p: PhantomData<Rc<()>>,
}

/// Mock timer that starts with the current time.
#[derive(Debug)]
pub struct MockNow {
    inner: Inner,
}

/// Mock parker
#[derive(Debug)]
pub struct MockPark {
    inner: Inner,
    _p: PhantomData<Rc<()>>,
}

/// Mock unparker
#[derive(Debug)]
pub struct MockUnpark {
    inner: Inner,
}

type Inner = Arc<Mutex<State>>;

#[derive(Debug)]
struct State {
    base: Instant,
    advance: Duration,
    unparked: bool,
    park_for: Option<Duration>,
}

/// Convert into a timeout useful for parking
pub trait IntoTimeout {
    /// Convert ourself into a timeout
    fn into_timeout(self) -> Option<Duration>;
}

impl IntoTimeout for Option<Duration> {
    fn into_timeout(self) -> Self {
        self
    }
}

impl IntoTimeout for Duration {
    fn into_timeout(self) -> Option<Duration> {
        Some(self)
    }
}

/// Turn the timer state once Internally calls [`Timer::turn`][timer]
///
/// [timer]: ../../tokio_timer/timer/struct.Timer.html#method.turn
pub fn turn<T: IntoTimeout>(timer: &mut Timer<MockPark>, duration: T) {
    timer.turn(duration.into_timeout()).unwrap();
}

/// Advance the timer the specified amount
pub fn advance(timer: &mut Timer<MockPark>, duration: Duration) {
    let inner = timer.get_park().inner.clone();
    let deadline = inner.lock().unwrap().now() + duration;

    while inner.lock().unwrap().now() < deadline {
        let dur = deadline - inner.lock().unwrap().now();
        turn(timer, dur);
    }
}

/// Enter a mocked timer context that starts with the current time
pub fn mocked<F, R>(f: F) -> R
where
    F: FnOnce(&mut Timer<MockPark>, &mut MockTime) -> R,
{
    mocked_with(Instant::now(), f)
}

/// Enter a mocked timer context that starts with the provided instant
pub fn mocked_with<F, R>(now: Instant, f: F) -> R
where
    F: FnOnce(&mut Timer<MockPark>, &mut MockTime) -> R,
{
    let mut time = MockTime::new(now);
    let park = time.mock_park();
    let now = ::tokio_timer::clock::Clock::new_with_now(time.mock_now());

    let mut enter = ::tokio_executor::enter().unwrap();

    ::tokio_timer::clock::with_default(&now, &mut enter, |enter| {
        let mut timer = Timer::new(park);
        let handle = timer.handle();

        ::tokio_timer::with_default(&handle, enter, |_| {
            lazy(|| Ok::<_, ()>(f(&mut timer, &mut time)))
                .wait()
                .unwrap()
        })
    })
}

impl MockTime {
    /// Create a mock timer with the passed instant.
    pub fn new(now: Instant) -> MockTime {
        let state = State {
            base: now,
            advance: Duration::default(),
            unparked: false,
            park_for: None,
        };

        MockTime {
            inner: Arc::new(Mutex::new(state)),
            _p: PhantomData,
        }
    }

    /// Create a mock timer with the current time.
    pub fn mock_now(&self) -> MockNow {
        let inner = self.inner.clone();
        MockNow { inner }
    }

    /// Get the mock parker from this timer.
    pub fn mock_park(&self) -> MockPark {
        let inner = self.inner.clone();
        MockPark {
            inner,
            _p: PhantomData,
        }
    }

    /// Get the current mocked instant.
    pub fn now(&self) -> Instant {
        self.inner.lock().unwrap().now()
    }

    /// Returns the total amount of time the time has been advanced.
    pub fn advanced(&self) -> Duration {
        self.inner.lock().unwrap().advance
    }

    /// Advance the mock timer by the provided duration.
    pub fn advance(&self, duration: Duration) {
        let mut inner = self.inner.lock().unwrap();
        inner.advance(duration);
    }

    /// The next call to park_timeout will be for this duration, regardless of
    /// the timeout passed to `park_timeout`.
    pub fn park_for(&self, duration: Duration) {
        self.inner.lock().unwrap().park_for = Some(duration);
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

impl State {
    fn now(&self) -> Instant {
        self.base + self.advance
    }

    fn advance(&mut self, duration: Duration) {
        self.advance += duration;
    }
}
