use crate::park::{Park, Unpark};
use crate::time::driver::{self, Driver};
use crate::time::{Clock, Duration, Instant};

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Run the provided closure with a `MockClock` that starts at the current time.
pub(crate) fn mock<F, R>(f: F) -> R
where
    F: FnOnce(&mut Handle) -> R,
{
    let mut mock = MockClock::new();
    mock.enter(f)
}

/// Mock clock for use with `tokio-timer` futures.
///
/// A mock timer that is able to advance and wake after a
/// certain duration.
#[derive(Debug)]
pub(crate) struct MockClock {
    time: MockTime,
    clock: Clock,
}

/// A handle to the `MockClock`.
#[derive(Debug)]
pub(crate) struct Handle {
    timer: Driver<MockPark>,
    time: MockTime,
    clock: Clock,
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
    clock: Clock,
    unparked: bool,
    park_for: Option<Duration>,
}

impl MockClock {
    /// Create a new `MockClock` with the current time.
    pub(crate) fn new() -> Self {
        let clock = Clock::new_frozen();
        let time = MockTime::new(clock.clone());

        MockClock { time, clock }
    }

    /// Enter the `MockClock` context.
    pub(crate) fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Handle) -> R,
    {
        self.clock.enter(|| {
            let park = self.time.mock_park();
            let timer = Driver::new(park, self.clock.clone());
            let handle = timer.handle();
            let _e = driver::set_default(&handle);

            let time = self.time.clone();

            let mut handle = Handle::new(timer, time, self.clock.clone());
            f(&mut handle)
            // lazy(|| Ok::<_, ()>(f(&mut handle))).wait().unwrap()
        })
    }
}

impl Default for MockClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Handle {
    pub(self) fn new(timer: Driver<MockPark>, time: MockTime, clock: Clock) -> Self {
        Handle { timer, time, clock }
    }

    /// Turn the internal timer and mock park for the provided duration.
    pub(crate) fn turn(&mut self) {
        self.timer.park().unwrap();
    }

    /// Turn the internal timer and mock park for the provided duration.
    pub(crate) fn turn_for(&mut self, duration: Duration) {
        self.timer.park_timeout(duration).unwrap();
    }

    /// Advance the `MockClock` by the provided duration.
    pub(crate) fn advance(&mut self, duration: Duration) {
        let now = Instant::now();
        let end = now + duration;

        while Instant::now() < end {
            self.turn_for(end - Instant::now());
        }
    }

    /// Returns the total amount of time the time has been advanced.
    pub(crate) fn advanced(&self) -> Duration {
        self.clock.advanced()
    }

    /// Get the currently mocked time
    pub(crate) fn now(&mut self) -> Instant {
        self.time.now()
    }

    /// Turn the internal timer once, but force "parking" for `duration` regardless of any pending
    /// timeouts
    pub(crate) fn park_for(&mut self, duration: Duration) {
        self.time.inner.lock().unwrap().park_for = Some(duration);
        self.turn()
    }
}

impl MockTime {
    pub(crate) fn new(clock: Clock) -> MockTime {
        let state = State {
            clock,
            unparked: false,
            park_for: None,
        };

        MockTime {
            inner: Arc::new(Mutex::new(state)),
            _pd: PhantomData,
        }
    }

    pub(crate) fn mock_park(&self) -> MockPark {
        let inner = self.inner.clone();
        MockPark {
            inner,
            _pd: PhantomData,
        }
    }

    pub(crate) fn now(&self) -> Instant {
        Instant::now()
    }
}

impl State {}

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
        inner.clock.advance(duration);

        Ok(())
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        let mut inner = self.inner.lock().unwrap();

        if let Some(duration) = inner.park_for.take() {
            inner.clock.advance(duration);
        } else {
            inner.clock.advance(duration);
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
