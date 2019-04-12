//! TODO

use futures::{future::lazy, Future};
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio_executor::park::{Park, Unpark};
use tokio_timer::{clock, Timer};

/// Mock timber
///
/// A mock timer that is able to advance and wake after a
/// certain duration.
#[derive(Debug)]
pub struct Clock {
    time: MockTime,
    clock: clock::Clock,
}

/// REMIND ME TO ADD THIS
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

impl Clock {
    /// Create a new `MockTimer` with the current time.
    pub fn new() -> Self {
        let time = MockTime::new(Instant::now());
        let clock = clock::Clock::new_with_now(time.mock_now());

        Clock { time, clock }
    }

    /// REMIND ME TO FIX THIS
    pub fn enter<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Handle) -> R,
    {
        let mut enter = tokio_executor::enter().unwrap();

        tokio_timer::clock::with_default(&self.clock, &mut enter, |enter| {
            let park = self.time.mock_park();
            let timer = Timer::new(park);
            let handle = timer.handle();
            let time = self.time.clone();

            tokio_timer::with_default(&handle, enter, |_| {
                let mut handle = Handle::new(timer, time);
                lazy(|| Ok::<_, ()>(f(&mut handle))).wait().unwrap()
            })
        })
    }
}

impl Handle {
    pub(self) fn new(timer: Timer<MockPark>, time: MockTime) -> Self {
        Handle { timer, time }
    }

    /// REMIND ME
    pub fn turn(&mut self, duration: Option<Duration>) {
        self.timer.turn(duration).unwrap();
    }

    /// REMIND ME
    pub fn advance(&mut self, duration: Duration) {
        let inner = self.timer.get_park().inner.clone();
        let deadline = inner.lock().unwrap().now() + duration;

        while inner.lock().unwrap().now() < deadline {
            let dur = deadline - inner.lock().unwrap().now();
            self.turn(Some(dur));
        }
    }

    /// Get the currently mocked time
    pub fn now(&mut self) -> Instant {
        self.time.now()
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

    // pub(crate) fn advanced(&self) -> Duration {
    //     self.inner.lock().unwrap().advance
    // }

    // pub(crate) fn advance(&self, duration: Duration) {
    //     let mut inner = self.inner.lock().unwrap();
    //     inner.advance(duration);
    // }

    // pub(crate) fn park_for(&self, duration: Duration) {
    //     self.inner.lock().unwrap().park_for = Some(duration);
    // }
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

impl clock::Now for MockNow {
    fn now(&self) -> Instant {
        self.inner.lock().unwrap().now()
    }
}
