#![allow(unused_macros, unused_imports, dead_code)]

use tokio_executor::park::{Park, Unpark};
use tokio_timer::timer::{Timer, Now};

use futures::future::{lazy, Future};

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};

macro_rules! assert_ready {
    ($f:expr) => {
        assert!($f.poll().unwrap().is_ready());
    };
    ($f:expr, $expect:expr) => {
        assert_eq!($f.poll().unwrap(), ::futures::Async::Ready($expect));
    };
}

macro_rules! assert_not_ready {
    ($f:expr) => {
        assert!(!$f.poll().unwrap().is_ready());
    }
}

macro_rules! assert_elapsed {
    ($f:expr) => {
        assert!($f.poll().unwrap_err().is_elapsed());
    }
}

#[derive(Debug)]
pub struct MockTime {
    inner: Inner,
    _p: PhantomData<Rc<()>>,
}

#[derive(Debug)]
pub struct MockNow {
    inner: Inner,
    _p: PhantomData<Rc<()>>,
}

#[derive(Debug)]
pub struct MockPark {
    inner: Inner,
    _p: PhantomData<Rc<()>>,
}

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

pub fn ms(num: u64) -> Duration {
    Duration::from_millis(num)
}

pub trait IntoTimeout {
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

/// Turn the timer state once
pub fn turn<T: IntoTimeout>(timer: &mut Timer<MockPark, MockNow>, duration: T) {
    timer.turn(duration.into_timeout()).unwrap();
}

/// Advance the timer the specified amount
pub fn advance(timer: &mut Timer<MockPark, MockNow>, duration: Duration) {
    let inner = timer.get_park().inner.clone();
    let deadline = inner.lock().unwrap().now() + duration;

    while inner.lock().unwrap().now() < deadline {
        let dur = deadline - inner.lock().unwrap().now();
        turn(timer, dur);
    }
}

pub fn mocked<F, R>(f: F) -> R
where F: FnOnce(&mut Timer<MockPark, MockNow>, &mut MockTime) -> R
{
    mocked_with_now(Instant::now(), f)
}

pub fn mocked_with_now<F, R>(now: Instant, f: F) -> R
where F: FnOnce(&mut Timer<MockPark, MockNow>, &mut MockTime) -> R
{
    let mut time = MockTime::new(now);
    let park = time.mock_park();
    let now = time.mock_now();

    let mut timer = Timer::new_with_now(park, now);
    let handle = timer.handle();

    let mut enter = ::tokio_executor::enter().unwrap();

    ::tokio_timer::with_default(&handle, &mut enter, |_| {
        lazy(|| {
            Ok::<_, ()>(f(&mut timer, &mut time))
        }).wait().unwrap()
    })
}

impl MockTime {
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

    pub fn mock_now(&self) -> MockNow {
        let inner = self.inner.clone();
        MockNow {
            inner,
            _p: PhantomData,
        }
    }

    pub fn mock_park(&self) -> MockPark {
        let inner = self.inner.clone();
        MockPark {
            inner,
            _p: PhantomData,
        }
    }

    pub fn now(&self) -> Instant {
        self.inner.lock().unwrap().now()
    }

    /// Returns the total amount of time the time has been advanced.
    pub fn advanced(&self) -> Duration {
        self.inner.lock().unwrap().advance
    }

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

        let duration = inner.park_for.take()
            .expect("call park_for first");

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
    fn now(&mut self) -> Instant {
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
