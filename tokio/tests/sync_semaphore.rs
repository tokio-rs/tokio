#![warn(rust_2018_idioms)]

use tokio::sync::semaphore::{Permit, Semaphore};
use tokio_test::task::MockTask;
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok};

#[test]
fn available_permits() {
    let mut t1 = MockTask::new();

    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();
    assert!(!permit.is_acquired());

    assert_ready_ok!(t1.enter(|cx| permit.poll_acquire(cx, &s)));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready_ok!(t1.enter(|cx| permit.poll_acquire(cx, &s)));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());
}

#[test]
fn unavailable_permits() {
    let mut t1 = MockTask::new();
    let mut t2 = MockTask::new();
    let s = Semaphore::new(1);

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();

    // Acquire the first permit
    assert_ready_ok!(t1.enter(|cx| permit_1.poll_acquire(cx, &s)));
    assert_eq!(s.available_permits(), 0);

    t2.enter(|cx| {
        // Try to acquire the second permit
        assert_pending!(permit_2.poll_acquire(cx, &s));
    });

    permit_1.release(&s);

    assert_eq!(s.available_permits(), 0);
    assert!(t2.is_woken());
    assert_ready_ok!(t2.enter(|cx| permit_2.poll_acquire(cx, &s)));

    permit_2.release(&s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn zero_permits() {
    let mut t1 = MockTask::new();

    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);

    let mut permit = Permit::new();

    // Try to acquire the permit
    t1.enter(|cx| {
        assert_pending!(permit.poll_acquire(cx, &s));
    });

    s.add_permits(1);

    assert!(t1.is_woken());
    assert_ready_ok!(t1.enter(|cx| permit.poll_acquire(cx, &s)));
}

#[test]
#[should_panic]
fn validates_max_permits() {
    use std::usize;
    Semaphore::new((usize::MAX >> 2) + 1);
}

#[test]
fn close_semaphore_prevents_acquire() {
    let mut t1 = MockTask::new();

    let s = Semaphore::new(1);
    s.close();

    assert_eq!(1, s.available_permits());

    let mut permit = Permit::new();

    assert_ready_err!(t1.enter(|cx| permit.poll_acquire(cx, &s)));
    assert_eq!(1, s.available_permits());
}

#[test]
fn close_semaphore_notifies_permit1() {
    let mut t1 = MockTask::new();

    let s = Semaphore::new(0);
    let mut permit = Permit::new();

    assert_pending!(t1.enter(|cx| permit.poll_acquire(cx, &s)));

    s.close();

    assert!(t1.is_woken());
    assert_ready_err!(t1.enter(|cx| permit.poll_acquire(cx, &s)));
}

#[test]
fn close_semaphore_notifies_permit2() {
    let mut t1 = MockTask::new();
    let mut t2 = MockTask::new();
    let mut t3 = MockTask::new();
    let mut t4 = MockTask::new();

    let s = Semaphore::new(2);

    let mut permit1 = Permit::new();
    let mut permit2 = Permit::new();
    let mut permit3 = Permit::new();
    let mut permit4 = Permit::new();

    // Acquire a couple of permits
    assert_ready_ok!(t1.enter(|cx| permit1.poll_acquire(cx, &s)));
    assert_ready_ok!(t2.enter(|cx| permit2.poll_acquire(cx, &s)));

    assert_pending!(t3.enter(|cx| permit3.poll_acquire(cx, &s)));
    assert_pending!(t4.enter(|cx| permit4.poll_acquire(cx, &s)));

    s.close();

    assert!(t3.is_woken());
    assert!(t4.is_woken());

    assert_ready_err!(t3.enter(|cx| permit3.poll_acquire(cx, &s)));
    assert_ready_err!(t4.enter(|cx| permit4.poll_acquire(cx, &s)));

    assert_eq!(0, s.available_permits());

    permit1.release(&s);

    assert_eq!(1, s.available_permits());

    assert_ready_err!(t1.enter(|cx| permit1.poll_acquire(cx, &s)));

    permit2.release(&s);

    assert_eq!(2, s.available_permits());
}
