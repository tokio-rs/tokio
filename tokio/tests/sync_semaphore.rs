#![warn(rust_2018_idioms)]

use tokio::sync::semaphore::{Permit, Semaphore};
use tokio_test::task;
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok};

#[test]
fn available_permits() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = task::spawn(Permit::new());
    assert!(!permit.is_acquired());

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());
}

#[test]
fn unavailable_permits() {
    let s = Semaphore::new(1);

    let mut permit_1 = task::spawn(Permit::new());
    let mut permit_2 = task::spawn(Permit::new());

    // Acquire the first permit
    assert_ready_ok!(permit_1.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_eq!(s.available_permits(), 0);

    permit_2.enter(|cx, mut p| {
        // Try to acquire the second permit
        assert_pending!(p.poll_acquire(cx, &s));
    });

    permit_1.release(&s);

    assert_eq!(s.available_permits(), 0);
    assert!(permit_2.is_woken());
    assert_ready_ok!(permit_2.enter(|cx, mut p| p.poll_acquire(cx, &s)));

    permit_2.release(&s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn zero_permits() {
    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);

    let mut permit = task::spawn(Permit::new());

    // Try to acquire the permit
    permit.enter(|cx, mut p| {
        assert_pending!(p.poll_acquire(cx, &s));
    });

    s.add_permits(1);

    assert!(permit.is_woken());
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, &s)));
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

    let mut permit = task::spawn(Permit::new());

    assert_ready_err!(permit.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_eq!(1, s.available_permits());
}

#[test]
fn close_semaphore_notifies_permit1() {
    let s = Semaphore::new(0);
    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, &s)));

    s.close();

    assert!(permit.is_woken());
    assert_ready_err!(permit.enter(|cx, mut p| p.poll_acquire(cx, &s)));
}

#[test]
fn close_semaphore_notifies_permit2() {
    let s = Semaphore::new(2);

    let mut permit1 = task::spawn(Permit::new());
    let mut permit2 = task::spawn(Permit::new());
    let mut permit3 = task::spawn(Permit::new());
    let mut permit4 = task::spawn(Permit::new());

    // Acquire a couple of permits
    assert_ready_ok!(permit1.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_ready_ok!(permit2.enter(|cx, mut p| p.poll_acquire(cx, &s)));

    assert_pending!(permit3.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_pending!(permit4.enter(|cx, mut p| p.poll_acquire(cx, &s)));

    s.close();

    assert!(permit3.is_woken());
    assert!(permit4.is_woken());

    assert_ready_err!(permit3.enter(|cx, mut p| p.poll_acquire(cx, &s)));
    assert_ready_err!(permit4.enter(|cx, mut p| p.poll_acquire(cx, &s)));

    assert_eq!(0, s.available_permits());

    permit1.release(&s);

    assert_eq!(1, s.available_permits());

    assert_ready_err!(permit1.enter(|cx, mut p| p.poll_acquire(cx, &s)));

    permit2.release(&s);

    assert_eq!(2, s.available_permits());
}
