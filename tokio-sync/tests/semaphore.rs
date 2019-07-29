extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_mock_task::*;
use tokio_sync::semaphore::{Permit, Semaphore};

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::NotReady) => {}
            Ok(futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
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

    permit_2.release(&s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn zero_permits() {
    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);

    let mut permit = Permit::new();
    let mut task = MockTask::new();

    // Try to acquire the permit
    task.enter(|| {
        assert_not_ready!(permit.poll_acquire(&s));
    });

    s.add_permits(1);

    assert!(task.is_notified());
    assert_ready!(permit.poll_acquire(&s));
}

#[test]
#[should_panic]
fn validates_max_permits() {
    use std::usize;
    Semaphore::new((usize::MAX >> 2) + 1);
}

#[test]
fn close_semaphore_prevents_acquire() {
    let s = Semaphore::new(1);
    s.close();

    assert_eq!(1, s.available_permits());

    let mut permit = Permit::new();

    assert!(permit.poll_acquire(&s).is_err());
    assert_eq!(1, s.available_permits());
}

#[test]
fn close_semaphore_notifies_permit1() {
    let s = Semaphore::new(0);

    let mut permit = Permit::new();
    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(permit.poll_acquire(&s));
    });

    s.close();

    assert!(task.is_notified());
    assert!(permit.poll_acquire(&s).is_err());
}

#[test]
fn close_semaphore_notifies_permit2() {
    let s = Semaphore::new(2);

    let mut permit1 = Permit::new();
    let mut permit2 = Permit::new();
    let mut permit3 = Permit::new();
    let mut permit4 = Permit::new();

    // Acquire a couple of permits
    assert_ready!(permit1.poll_acquire(&s));
    assert_ready!(permit2.poll_acquire(&s));

    let mut task1 = MockTask::new();
    let mut task2 = MockTask::new();

    task1.enter(|| {
        assert_not_ready!(permit3.poll_acquire(&s));
    });

    task2.enter(|| {
        assert_not_ready!(permit4.poll_acquire(&s));
    });

    s.close();

    assert!(task1.is_notified());
    assert!(task2.is_notified());

    assert!(permit3.poll_acquire(&s).is_err());
    assert!(permit4.poll_acquire(&s).is_err());

    assert_eq!(0, s.available_permits());

    permit1.release(&s);

    assert_eq!(1, s.available_permits());

    assert!(permit1.poll_acquire(&s).is_err());

    permit2.release(&s);

    assert_eq!(2, s.available_permits());
}
