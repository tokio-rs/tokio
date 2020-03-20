use crate::sync::batch_semaphore::Semaphore;
use tokio_test::*;

#[test]
fn poll_acquire_one_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = assert_ready_ok!(task::spawn(s.acquire(1)).poll());
    assert_eq!(s.available_permits(), 99);

    // Polling again on the same permit does not claim a new permit
    assert_ready_ok!(task::spawn(permit.acquire(1, &s)).poll());
    assert_eq!(s.available_permits(), 99);
}

#[test]
fn poll_acquire_many_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = assert_ready_ok!(task::spawn(s.acquire(5)).poll());
    assert_eq!(s.available_permits(), 95);

    // Polling again on the same permit does not claim a new permit
    assert_ready_ok!(task::spawn(permit.acquire(1, &s)).poll());
    assert_eq!(s.available_permits(), 95);

    assert_ready_ok!(task::spawn(permit.acquire(5, &s)).poll());
    assert_eq!(s.available_permits(), 95);

    // Polling for a larger number of permits acquires more
    assert_ready_ok!(task::spawn(permit.acquire(8, &s)).poll());
    assert_eq!(s.available_permits(), 92);
}

#[test]
fn try_acquire_one_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = assert_ok!(s.try_acquire(1));
    assert_eq!(s.available_permits(), 99);

    // Polling again on the same permit does not claim a new permit
    assert_ok!(permit.try_acquire(1, &s));
    assert_eq!(s.available_permits(), 99);
}

#[test]
fn try_acquire_many_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = assert_ok!(s.try_acquire(5));
    assert_eq!(s.available_permits(), 95);
    // Polling again on the same permit does not claim a new permit
    assert_ok!(permit.try_acquire(5, &s));
    assert_eq!(s.available_permits(), 95);
}

#[test]
fn poll_acquire_one_unavailable() {
    let s = Semaphore::new(1);

    // Acquire the first permit
    let mut permit_1 = assert_ready_ok!(task::spawn(s.acquire(1)).poll());
    assert_eq!(s.available_permits(), 0);

    let mut acquire_2 = task::spawn(s.acquire(1));
    // Try to acquire the second permit
    assert_pending!(acquire_2.poll());
    assert_eq!(s.available_permits(), 0);

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 0);
    assert!(acquire_2.is_woken());
    let mut permit_2 = assert_ready_ok!(acquire_2.poll());
    assert_eq!(s.available_permits(), 0);

    assert_ready_ok!(task::spawn(permit_2.acquire(1, &s)).poll());

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn forget_acquired() {
    let s = Semaphore::new(1);

    // Polling for a permit succeeds immediately
    let mut permit = assert_ready_ok!(task::spawn(s.acquire(1)).poll());

    assert_eq!(s.available_permits(), 0);

    permit.forget(1);
    assert_eq!(s.available_permits(), 0);
}

#[test]
fn poll_acquire_many_unavailable() {
    let s = Semaphore::new(5);

    // Acquire the first permit
    let mut permit_1 = assert_ready_ok!(task::spawn(s.acquire(1)).poll());
    assert_eq!(s.available_permits(), 4);

    // Try to acquire the second permit
    let mut acquire_2 = task::spawn(s.acquire(5));
    assert_pending!(acquire_2.poll());
    assert_eq!(s.available_permits(), 0);

    // Try to acquire the third permit
    let mut acquire_3 = task::spawn(s.acquire(3));
    assert_pending!(acquire_3.poll());
    assert_eq!(s.available_permits(), 0);

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 0);
    assert!(acquire_2.is_woken());
    let mut permit_2 = assert_ready_ok!(acquire_2.poll());

    assert!(!acquire_3.is_woken());
    assert_eq!(s.available_permits(), 0);

    drop(acquire_2); // drop the acquire future so we can now
    permit_2.release(1, &s);
    assert!(!acquire_3.is_woken());
    assert_eq!(s.available_permits(), 0);

    permit_2.release(2, &s);
    assert!(acquire_3.is_woken());

    assert_ready_ok!(acquire_3.poll());
}

#[test]
fn try_acquire_one_unavailable() {
    let s = Semaphore::new(1);

    // Acquire the first permit
    let mut permit_1 = assert_ok!(s.try_acquire(1));
    assert_eq!(s.available_permits(), 0);

    assert_err!(s.try_acquire(1));

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 1);
    let mut permit_2 = assert_ok!(s.try_acquire(1));

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn try_acquire_many_unavailable() {
    let s = Semaphore::new(5);

    // Acquire the first permit
    let mut permit_1 = assert_ok!(s.try_acquire(1));
    assert_eq!(s.available_permits(), 4);

    assert_err!(s.try_acquire(5));

    permit_1.release(1, &s);
    assert_eq!(s.available_permits(), 5);

    let mut permit_2 = assert_ok!(s.try_acquire(5));

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 2);
}

#[test]
fn poll_acquire_one_zero_permits() {
    let s = Semaphore::new(0);
    assert_eq!(s.available_permits(), 0);

    // Try to acquire the permit
    let mut acquire = task::spawn(s.acquire(1));
    assert_pending!(acquire.poll());

    s.add_permits(1);

    assert!(acquire.is_woken());
    assert_ready_ok!(acquire.poll());
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

    assert_ready_err!(task::spawn(s.acquire(1)).poll());
    assert_eq!(5, s.available_permits());

    assert_ready_err!(task::spawn(s.acquire(1)).poll());
    assert_eq!(5, s.available_permits());
}

#[test]
fn close_semaphore_notifies_permit1() {
    let s = Semaphore::new(0);
    let mut acquire = task::spawn(s.acquire(1));

    assert_pending!(acquire.poll());

    s.close();

    assert!(acquire.is_woken());
    assert_ready_err!(acquire.poll());
}

#[test]
fn close_semaphore_notifies_permit2() {
    let s = Semaphore::new(2);

    // Acquire a couple of permits
    let mut permit1 = assert_ready_ok!(task::spawn(s.acquire(1)).poll());
    let mut permit2 = assert_ready_ok!(task::spawn(s.acquire(1)).poll());

    let mut acquire3 = task::spawn(s.acquire(1));
    let mut acquire4 = task::spawn(s.acquire(1));
    assert_pending!(acquire3.poll());
    assert_pending!(acquire4.poll());

    s.close();

    assert!(acquire3.is_woken());
    assert!(acquire4.is_woken());

    assert_ready_err!(acquire3.poll());
    assert_ready_err!(acquire4.poll());

    assert_eq!(0, s.available_permits());

    permit1.release(1, &s);

    assert_eq!(1, s.available_permits());

    assert_ready_err!(task::spawn(permit1.acquire(1, &s)).poll());

    permit2.release(1, &s);

    assert_eq!(2, s.available_permits());
}

#[test]
fn cancel_acquire_releases_permits() {
    let s = Semaphore::new(10);
    let _permit1 = s.try_acquire(4).expect("uncontended try_acquire succeeds");
    assert_eq!(6, s.available_permits());

    let mut acquire = task::spawn(s.acquire(8));
    assert_pending!(acquire.poll());

    assert_eq!(0, s.available_permits());
    drop(acquire);

    assert_eq!(6, s.available_permits());
    assert_ok!(s.try_acquire(6));
}
