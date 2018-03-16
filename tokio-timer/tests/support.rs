use tokio_executor::park::{Park, Unpark};
use tokio_timer::{Timer, Now};

use futures::future::{lazy, Future};

use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Instant, Duration};

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
}

pub fn ms(num: u64) -> Duration {
    Duration::from_millis(num)
}

pub fn turn(timer: &mut Timer<MockPark, MockNow>, duration: Duration) {
    timer.turn(Some(duration)).unwrap();
}

pub fn mocked<F, R>(f: F) -> R
where F: FnOnce(&mut Timer<MockPark, MockNow>, &mut MockTime) -> R
{
    let mut time = MockTime::new();
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
    pub fn new() -> MockTime {
        let state = State {
            base: Instant::now(),
            advance: Duration::default(),
            unparked: false,
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
}

impl Park for MockPark {
    type Unpark = MockUnpark;
    type Error = ();

    fn unpark(&self) -> Self::Unpark {
        let inner = self.inner.clone();
        MockUnpark { inner }
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        unimplemented!();
    }

    fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
        self.inner.lock().unwrap().advance(duration);
        Ok(())
    }
}

impl Unpark for MockUnpark {
    fn unpark(&self) {
        self.inner.lock().unwrap().unparked = true;
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
