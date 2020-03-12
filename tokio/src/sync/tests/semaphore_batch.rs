use crate::sync::batch_semaphore::{Permit, Semaphore};
use tokio_test::*;

#[test]
fn poll_acquire_one_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();
    assert!(!permit.is_acquired());

    assert_ready_ok!(task::spawn(permit.acquire(1, &s)).poll());
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready_ok!(task::spawn(permit.acquire(1, &s)).poll());
    assert_eq!(s.available_permits(), 99);
    assert!(permit.is_acquired());
}

#[test]
fn poll_acquire_many_available() {
    let s = Semaphore::new(100);
    assert_eq!(s.available_permits(), 100);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();
    assert!(!permit.is_acquired());

    assert_ready_ok!(task::spawn(permit.acquire(5, &s)).poll());
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    // Polling again on the same waiter does not claim a new permit
    assert_ready_ok!(task::spawn(permit.acquire(1, &s)).poll());
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    assert_ready_ok!(task::spawn(permit.acquire(5, &s)).poll());
    assert_eq!(s.available_permits(), 95);
    assert!(permit.is_acquired());

    // Polling for a larger number of permits acquires more
    assert_ready_ok!(task::spawn(permit.acquire(8, &s)).poll());
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

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();

    {
        let mut acquire_1 = task::spawn(permit_1.acquire(1, &s));
        // Acquire the first permit
        assert_ready_ok!(acquire_1.poll());
        assert_eq!(s.available_permits(), 0);
    }

    {
        let mut acquire_2 = task::spawn(permit_2.acquire(1, &s));
        // Try to acquire the second permit
        assert_pending!(acquire_2.poll());
        assert_eq!(s.available_permits(), 0);

        permit_1.release(1, &s);

        assert_eq!(s.available_permits(), 0);
        assert!(acquire_2.is_woken());
        assert_ready_ok!(acquire_2.poll());
        assert_eq!(s.available_permits(), 0);
    }

    assert_ready_ok!(task::spawn(permit_2.acquire(1, &s)).poll());

    permit_2.release(1, &s);
    assert_eq!(s.available_permits(), 1);
}

#[test]
fn forget_acquired() {
    let s = Semaphore::new(1);

    // Polling for a permit succeeds immediately
    let mut permit = Permit::new();

    assert_ready_ok!(task::spawn(permit.acquire(1, &s)).poll());

    assert_eq!(s.available_permits(), 0);

    permit.forget(1);
    assert_eq!(s.available_permits(), 0);
}

// #[test]
// fn forget_waiting() {
//     let s = Semaphore::new(0);

//     // Polling for a permit succeeds immediately
//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 1, &s)));

//     assert_eq!(s.available_permits(), 0);

//     permit.forget(1);

//     s.add_permits(1);

//     assert!(!permit.is_woken());
//     assert_eq!(s.available_permits(), 1);
// }

#[test]
fn poll_acquire_many_unavailable() {
    let s = Semaphore::new(5);

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();
    let mut permit_3 = Permit::new();

    // Acquire the first permit
    assert_ready_ok!(task::spawn(permit_1.acquire(1, &s)).poll());
    assert_eq!(s.available_permits(), 4);

    // Try to acquire the second permit
    let mut acquire_2 = task::spawn(permit_2.acquire(5, &s));
    assert_pending!(acquire_2.poll());
    assert_eq!(s.available_permits(), 0);

    // Try to acquire the third permit
    let mut acquire_3 = task::spawn(permit_3.acquire(3, &s));
    assert_pending!(acquire_3.poll());
    assert_eq!(s.available_permits(), 0);

    permit_1.release(1, &s);

    assert_eq!(s.available_permits(), 0);
    assert!(acquire_2.is_woken());
    assert_ready_ok!(acquire_2.poll());

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

    let mut permit = Permit::new();

    // Try to acquire the permit
    let mut acquire = task::spawn(permit.acquire(1, &s));
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

    let mut permit_1 = Permit::new();
    let mut permit_2 = Permit::new();

    assert_ready_err!(task::spawn(permit_1.acquire(1, &s)).poll());
    assert_eq!(5, s.available_permits());

    assert_ready_err!(task::spawn(permit_2.acquire(1, &s)).poll());
    assert_eq!(5, s.available_permits());
}

#[test]
fn close_semaphore_notifies_permit1() {
    let s = Semaphore::new(0);
    let mut permit = Permit::new();
    let mut acquire = task::spawn(permit.acquire(1, &s));

    assert_pending!(acquire.poll());

    s.close();

    assert!(acquire.is_woken());
    assert_ready_err!(acquire.poll());
}

#[test]
fn close_semaphore_notifies_permit2() {
    let s = Semaphore::new(2);

    let mut permit1 = Permit::new();
    let mut permit2 = Permit::new();
    let mut permit3 = Permit::new();
    let mut permit4 = Permit::new();

    // Acquire a couple of permits
    assert_ready_ok!(task::spawn(permit1.acquire(1, &s)).poll());
    assert_ready_ok!(task::spawn(permit2.acquire(1, &s)).poll());

    let mut acquire3 = task::spawn(permit3.acquire(1, &s));
    let mut acquire4 = task::spawn(permit4.acquire(1, &s));
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

// #[test]
// fn poll_acquire_additional_permits_while_waiting_before_assigned() {
//     let s = Semaphore::new(1);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));
//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 3, &s)));

//     s.add_permits(1);
//     assert!(!permit.is_woken());

//     s.add_permits(1);
//     assert!(permit.is_woken());

//     assert_ready_ok!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 3, &s)));
// }

// #[test]
// fn try_acquire_additional_permits_while_waiting_before_assigned() {
//     let s = Semaphore::new(1);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));

//     assert_err!(permit.enter(|_, mut p| p.try_acquire(3, &s)));

//     s.add_permits(1);
//     assert!(permit.is_woken());

//     assert_ok!(permit.enter(|_, mut p| p.try_acquire(2, &s)));
// }

// #[test]
// fn poll_acquire_additional_permits_while_waiting_after_assigned_success() {
//     let s = Semaphore::new(1);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));

//     s.add_permits(2);

//     assert!(permit.is_woken());
//     assert_ready_ok!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 3, &s)));
// }

// #[test]
// fn poll_acquire_additional_permits_while_waiting_after_assigned_requeue() {
//     let s = Semaphore::new(1);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));

//     s.add_permits(2);

//     assert!(permit.is_woken());
//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 4, &s)));

//     s.add_permits(1);

//     assert!(permit.is_woken());
//     assert_ready_ok!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 4, &s)));
// }

// #[test]
// fn poll_acquire_fewer_permits_while_waiting() {
//     let s = Semaphore::new(1);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));
//     assert_eq!(s.available_permits(), 0);

//     assert_ready_ok!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 1, &s)));
//     assert_eq!(s.available_permits(), 0);
// }

// #[test]
// fn poll_acquire_fewer_permits_after_assigned() {
//     let s = Semaphore::new(1);

//     let mut permit1 = task::spawn(Box::new(Permit::new()));
//     let mut permit2 = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit1.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 5, &s)));
//     assert_eq!(s.available_permits(), 0);

//     assert_pending!(permit2.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 1, &s)));

//     s.add_permits(4);
//     assert!(permit1.is_woken());
//     assert!(!permit2.is_woken());

//     assert_ready_ok!(permit1.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 3, &s)));

//     assert!(permit2.is_woken());
//     assert_eq!(s.available_permits(), 1);

//     assert_ready_ok!(permit2.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 1, &s)));
// }

// #[test]
// fn forget_partial_1() {
//     let s = Semaphore::new(0);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));
//     s.add_permits(1);

//     assert_eq!(0, s.available_permits());

//     permit.release(1, &s);

//     assert_ready_ok!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 1, &s)));

//     assert_eq!(s.available_permits(), 0);
// }

// #[test]
// fn forget_partial_2() {
//     let s = Semaphore::new(0);

//     let mut permit = task::spawn(Box::new(Permit::new()));

//     assert_pending!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));
//     s.add_permits(1);

//     assert_eq!(0, s.available_permits());

//     permit.release(1, &s);

//     s.add_permits(1);

//     assert_ready_ok!(permit.enter(|cx, mut p| p.as_mut().poll_acquire(cx, 2, &s)));
//     assert_eq!(s.available_permits(), 0);
// }

#[test]
fn cancel_acquire_releases_permits() {
    let s = Semaphore::new(10);
    let mut permit1 = Permit::new();
    permit1
        .try_acquire(4, &s)
        .expect("uncontended try_acquire succeeds");
    assert_eq!(6, s.available_permits());

    let mut permit2 = Permit::new();
    let mut acquire = task::spawn(permit2.acquire(8, &s));
    assert_pending!(acquire.poll());

    assert_eq!(0, s.available_permits());
    drop(acquire);

    assert_eq!(6, s.available_permits());
    permit2
        .try_acquire(6, &s)
        .expect("should acquire successfully");
}
