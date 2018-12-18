extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_sync::{Semaphore, Permit};
use tokio_mock_task::*;

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }}
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::NotReady) => {},
            Ok(futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }}
}

#[test]
fn available_permits() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();
    assert!(!permit.is_acquired());

    assert_ready!(permit.poll_acquire(&s));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready!(permit.poll_acquire(&s));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());
}

#[test]
fn unavailable_permits() {
    let s = Semaphore::new(1);

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();

    // Acquire the first permit
    assert_ready!(permit_1.poll_acquire(&s));
    assert_eq!(s.available_permits(), 0);

    let mut task = MockTask::new();

    task.enter(|| {
        // Try to acquire the second permit
        assert_not_ready!(permit_2.poll_acquire(&s));
    });

    permit_1.release(&s);

    assert_eq!(s.available_permits(), 0);
    assert!(task.is_notified());
    assert_ready!(permit_2.poll_acquire(&s));
}

#[test]
#[ignore]
fn zero_permits() {
    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);
}
