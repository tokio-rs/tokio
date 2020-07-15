use crate::sync::semaphore_ll::{Permit, Semaphore};
use tokio_test::*;

#[test]
fn poll_acquire_one_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = task::spawn(Permit::new());
    assert!(!permit.is_acquired());

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());
}

#[test]
fn poll_acquire_many_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = task::spawn(Permit::new());
    assert!(!permit.is_acquired());

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 5, &s)));
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 5, &s)));
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    // Polling for a larger number of permits acquires more
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 8, &s)));
    assert_eq!(s.available_permits(), 92);
    assert!(permit.is_acquired());
}

#[test]
fn try_acquire_one_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();
    assert!(!permit.is_acquired());

    assert_ok!(permit.try_acquire(1, &s));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ok!(permit.try_acquire(1, &s));
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());
}

#[test]
fn try_acquire_many_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();
    assert!(!permit.is_acquired());

    assert_ok!(permit.try_acquire(5, &s));
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ok!(permit.try_acquire(5, &s));
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());
}

#[test]
fn poll_acquire_one_unavailable() {
    let s = Semaphore::new(1);

    let mut permit_1 = task::spawn(Permit::new());
    let mut permit_2 = task::spawn(Permit::new());

    // Acquire the first permit
    assert_ready_ok!(permit_1.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(s.available_permits(), 0);

    permit_2.enter(|cx, mut p| {
        // Try to acquire the second permit
        assert_pending!(p.poll_acquire(cx, 1, &s));
    });

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 0);
    assert!(permit_2.is_woken());
    assert_ready_ok!(permit_2.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn forget_acquired() {
    let s = Semaphore::new(1);

    // Polling for a permit succeeds immediately
    let mut permit = task::spawn(Permit::new());

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    assert_eq!(s.available_permits(), 0);

    permit.forget(1);
    assert_eq!(s.available_permits(), 0);
}

#[test]
fn forget_waiting() {
    let s = Semaphore::new(0);

    // Polling for a permit succeeds immediately
    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    assert_eq!(s.available_permits(), 0);

    permit.forget(1);

    s.add_permits(1);

    assert!(!permit.is_woken());
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn poll_acquire_many_unavailable() {
    let s = Semaphore::new(5);

    let mut permit_1 = task::spawn(Permit::new());
    let mut permit_2 = task::spawn(Permit::new());
    let mut permit_3 = task::spawn(Permit::new());

    // Acquire the first permit
    assert_ready_ok!(permit_1.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(s.available_permits(), 4);

    permit_2.enter(|cx, mut p| {
        // Try to acquire the second permit
        assert_pending!(p.poll_acquire(cx, 5, &s));
    });

    assert_eq!(s.available_permits(), 0);

    permit_3.enter(|cx, mut p| {
        // Try to acquire the third permit
        assert_pending!(p.poll_acquire(cx, 3, &s));
    });

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 0);
    assert!(permit_2.is_woken());
    assert_ready_ok!(permit_2.enter(|cx, mut p| p.poll_acquire(cx, 5, &s)));

    assert!(!permit_3.is_woken());
    assert_eq!(s.available_permits(), 0);

    permit_2.release(1, &s);
    assert!(!permit_3.is_woken());
    assert_eq!(s.available_permits(), 0);

    permit_2.release(2, &s);
    assert!(permit_3.is_woken());

    assert_ready_ok!(permit_3.enter(|cx, mut p| p.poll_acquire(cx, 3, &s)));
}

#[test]
fn try_acquire_one_unavailable() {
    let s = Semaphore::new(1);

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();

    // Acquire the first permit
    assert_ok!(permit_1.try_acquire(1, &s));
    assert_eq!(s.available_permits(), 0);

    assert_err!(permit_2.try_acquire(1, &s));

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 1);
    assert_ok!(permit_2.try_acquire(1, &s));

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn try_acquire_many_unavailable() {
    let s = Semaphore::new(5);

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();

    // Acquire the first permit
    assert_ok!(permit_1.try_acquire(1, &s));
    assert_eq!(s.available_permits(), 4);

    assert_err!(permit_2.try_acquire(5, &s));

    permit_1.release(1, &s);
    assert_eq!(s.available_permits(), 5);

    assert_ok!(permit_2.try_acquire(5, &s));

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 2);
}

#[test]
fn poll_acquire_one_zero_permits() {
    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);

    let mut permit = task::spawn(Permit::new());

    // Try to acquire the permit
    permit.enter(|cx, mut p| {
        assert_pending!(p.poll_acquire(cx, 1, &s));
    });

    s.add_permits(1);

    assert!(permit.is_woken());
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
}

#[test]
#[should_panic]
fn validates_max_permits() {
    use std::usize;
    Semaphore::new((usize::MAX >> 2) + 1);
}

#[test]
fn close_semaphore_prevents_acquire() {
    let s = Semaphore::new(5);
    s.close();

    assert_eq!(5, s.available_permits());

    let mut permit_1 = task::spawn(Permit::new());
    let mut permit_2 = task::spawn(Permit::new());

    assert_ready_err!(permit_1.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(5, s.available_permits());

    assert_ready_err!(permit_2.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));
    assert_eq!(5, s.available_permits());
}

#[test]
fn close_semaphore_notifies_permit1() {
    let s = Semaphore::new(0);
    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    s.close();

    assert!(permit.is_woken());
    assert_ready_err!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
}

#[test]
fn close_semaphore_notifies_permit2() {
    let s = Semaphore::new(2);

    let mut permit1 = task::spawn(Permit::new());
    let mut permit2 = task::spawn(Permit::new());
    let mut permit3 = task::spawn(Permit::new());
    let mut permit4 = task::spawn(Permit::new());

    // Acquire a couple of permits
    assert_ready_ok!(permit1.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_ready_ok!(permit2.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    assert_pending!(permit3.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_pending!(permit4.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    s.close();

    assert!(permit3.is_woken());
    assert!(permit4.is_woken());

    assert_ready_err!(permit3.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_ready_err!(permit4.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    assert_eq!(0, s.available_permits());

    permit1.release(1, &s);

    assert_eq!(1, s.available_permits());

    assert_ready_err!(permit1.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    permit2.release(1, &s);

    assert_eq!(2, s.available_permits());
}

#[test]
fn poll_acquire_additional_permits_while_waiting_before_assigned() {
    let s = Semaphore::new(1);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));
    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 3, &s)));

    s.add_permits(1);
    assert!(!permit.is_woken());

    s.add_permits(1);
    assert!(permit.is_woken());

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 3, &s)));
}

#[test]
fn try_acquire_additional_permits_while_waiting_before_assigned() {
    let s = Semaphore::new(1);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));

    assert_err!(permit.enter(|_, mut p| p.try_acquire(3, &s)));

    s.add_permits(1);
    assert!(permit.is_woken());

    assert_ok!(permit.enter(|_, mut p| p.try_acquire(2, &s)));
}

#[test]
fn poll_acquire_additional_permits_while_waiting_after_assigned_success() {
    let s = Semaphore::new(1);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));

    s.add_permits(2);

    assert!(permit.is_woken());
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 3, &s)));
}

#[test]
fn poll_acquire_additional_permits_while_waiting_after_assigned_requeue() {
    let s = Semaphore::new(1);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));

    s.add_permits(2);

    assert!(permit.is_woken());
    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 4, &s)));

    s.add_permits(1);

    assert!(permit.is_woken());
    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 4, &s)));
}

#[test]
fn poll_acquire_fewer_permits_while_waiting() {
    let s = Semaphore::new(1);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));
    assert_eq!(s.available_permits(), 0);

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
    assert_eq!(s.available_permits(), 0);
}

#[test]
fn poll_acquire_fewer_permits_after_assigned() {
    let s = Semaphore::new(1);

    let mut permit1 = task::spawn(Permit::new());
    let mut permit2 = task::spawn(Permit::new());

    assert_pending!(permit1.enter(|cx, mut p| p.poll_acquire(cx, 5, &s)));
    assert_eq!(s.available_permits(), 0);

    assert_pending!(permit2.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    s.add_permits(4);
    assert!(permit1.is_woken());
    assert!(!permit2.is_woken());

    assert_ready_ok!(permit1.enter(|cx, mut p| p.poll_acquire(cx, 3, &s)));

    assert!(permit2.is_woken());
    assert_eq!(s.available_permits(), 1);

    assert_ready_ok!(permit2.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));
}

#[test]
fn forget_partial_1() {
    let s = Semaphore::new(0);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));
    s.add_permits(1);

    assert_eq!(0, s.available_permits());

    permit.release(1, &s);

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 1, &s)));

    assert_eq!(s.available_permits(), 0);
}

#[test]
fn forget_partial_2() {
    let s = Semaphore::new(0);

    let mut permit = task::spawn(Permit::new());

    assert_pending!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));
    s.add_permits(1);

    assert_eq!(0, s.available_permits());

    permit.release(1, &s);

    s.add_permits(1);

    assert_ready_ok!(permit.enter(|cx, mut p| p.poll_acquire(cx, 2, &s)));
    assert_eq!(s.available_permits(), 0);
}
