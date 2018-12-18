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

#[test]
fn available_permits() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();

    assert_ready!(permit.poll_acquire(&s));
    assert_eq!(s.available_permits(), 99);

    // Polling again on the same waiter does not claim a new permit
    assert_ready!(permit.poll_acquire(&s));
    assert_eq!(s.available_permits(), 99);
}

#[test]
#[ignore]
fn zero_permits() {
    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);
}
