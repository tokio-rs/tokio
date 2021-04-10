use crate::sync::AtomicWaker;
use tokio_test::task;

use std::task::Waker;

trait AssertSend: Send {}
trait AssertSync: Send {}

impl AssertSend for AtomicWaker {}
impl AssertSync for AtomicWaker {}

impl AssertSend for Waker {}
impl AssertSync for Waker {}

#[test]
fn basic_usage() {
    let mut waker = task::spawn(AtomicWaker::new());

    waker.enter(|cx, waker| waker.register_by_ref(cx.waker()));
    waker.wake();

    assert!(waker.is_woken());
}

#[test]
fn wake_without_register() {
    let mut waker = task::spawn(AtomicWaker::new());
    waker.wake();

    // Registering should not result in a notification
    waker.enter(|cx, waker| waker.register_by_ref(cx.waker()));

    assert!(!waker.is_woken());
}

#[test]
fn atomic_waker_panic_safe() {
    use std::panic;
    use std::sync::Arc;
    use std::task::{RawWaker, RawWakerVTable, Wake, Waker};

    struct NonPanicking;

    impl Wake for NonPanicking {
        fn wake(self: Arc<Self>) {}
    }

    static VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| panic!("clone"),
        |_| unimplemented!("wake"),
        |_| unimplemented!("wake_by_ref"),
        |_| (),
    );
    let panicking = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };

    let atomic_waker = AtomicWaker::new();

    let panicking = panic::AssertUnwindSafe(&panicking);

    let result = panic::catch_unwind(|| {
        let panic::AssertUnwindSafe(panicking) = panicking;
        atomic_waker.register_by_ref(panicking);
    });

    let non_panicking = Waker::from(Arc::new(NonPanicking));

    assert!(result.is_err());
    assert!(atomic_waker.take_waker().is_none());

    atomic_waker.register_by_ref(&non_panicking);
    assert!(atomic_waker.take_waker().is_some());
}
