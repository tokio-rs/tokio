use crate::sync::CancellationToken;

use loom::sync::atomic::{AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::{future::block_on, thread};
use tokio_test::assert_ok;

#[test]
fn cancel_token() {
    loom::model(|| {
        let token = CancellationToken::new();
        let token1 = token.clone();

        let th1 = thread::spawn(move || {
            block_on(async {
                token1.cancelled().await;
            });
        });

        let th2 = thread::spawn(move || {
            token.cancel();
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

#[test]
fn cancel_token_owned() {
    loom::model(|| {
        let token = CancellationToken::new();
        let token1 = token.clone();

        let th1 = thread::spawn(move || {
            block_on(async {
                token1.cancelled_owned().await;
            });
        });

        let th2 = thread::spawn(move || {
            token.cancel();
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
    });
}

// Verifies that the lock-free `is_cancelled()` fast path establishes a
// happens-before relationship with `cancel()`. A data write performed before
// `cancel()` must be visible from another thread that has observed
// `is_cancelled() == true`.
#[test]
fn is_cancelled_publishes_writes_from_cancel() {
    loom::model(|| {
        let token = Arc::new(CancellationToken::new());
        let data = Arc::new(AtomicUsize::new(0));

        let writer_token = token.clone();
        let writer_data = data.clone();
        let writer = thread::spawn(move || {
            writer_data.store(42, Ordering::Relaxed);
            writer_token.cancel();
        });

        let reader_token = token.clone();
        let reader_data = data.clone();
        let reader = thread::spawn(move || {
            if reader_token.is_cancelled() {
                assert_eq!(reader_data.load(Ordering::Relaxed), 42);
            }
        });

        assert_ok!(writer.join());
        assert_ok!(reader.join());
    });
}

// Regression test for the lost-wakeup scenario: if a thread creates the
// `cancelled()` future and polls it (reading `is_cancelled` as `false`)
// concurrently with `cancel()` flipping the flag and calling
// `notify_waiters()`, the future must still complete. This would fail under
// loom if `is_cancelled()` used only `load(Acquire)` against a
// `store(Release)`, because that pair only orders one direction and would
// permit a missed wakeup.
#[test]
fn cancelled_future_no_lost_wakeup() {
    loom::model(|| {
        let token = Arc::new(CancellationToken::new());

        let cancel_token = token.clone();
        let canceller = thread::spawn(move || {
            cancel_token.cancel();
        });

        let awaiter_token = token.clone();
        let awaiter = thread::spawn(move || {
            block_on(async {
                awaiter_token.cancelled().await;
            });
        });

        assert_ok!(canceller.join());
        assert_ok!(awaiter.join());
    });
}

#[test]
fn cancel_with_child() {
    loom::model(|| {
        let token = CancellationToken::new();
        let token1 = token.clone();
        let token2 = token.clone();
        let child_token = token.child_token();

        let th1 = thread::spawn(move || {
            block_on(async {
                token1.cancelled().await;
            });
        });

        let th2 = thread::spawn(move || {
            token2.cancel();
        });

        let th3 = thread::spawn(move || {
            block_on(async {
                child_token.cancelled().await;
            });
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}

#[test]
fn drop_token_no_child() {
    loom::model(|| {
        let token = CancellationToken::new();
        let token1 = token.clone();
        let token2 = token.clone();

        let th1 = thread::spawn(move || {
            drop(token1);
        });

        let th2 = thread::spawn(move || {
            drop(token2);
        });

        let th3 = thread::spawn(move || {
            drop(token);
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}

// Temporarily disabled due to a false positive in loom -
// see https://github.com/tokio-rs/tokio/pull/7644#issuecomment-3328381344
#[ignore]
#[test]
fn drop_token_with_children() {
    loom::model(|| {
        let token1 = CancellationToken::new();
        let child_token1 = token1.child_token();
        let child_token2 = token1.child_token();

        let th1 = thread::spawn(move || {
            drop(token1);
        });

        let th2 = thread::spawn(move || {
            drop(child_token1);
        });

        let th3 = thread::spawn(move || {
            drop(child_token2);
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}

// Temporarily disabled due to a false positive in loom -
// see https://github.com/tokio-rs/tokio/pull/7644#issuecomment-3328381344
#[ignore]
#[test]
fn drop_and_cancel_token() {
    loom::model(|| {
        let token1 = CancellationToken::new();
        let token2 = token1.clone();
        let child_token = token1.child_token();

        let th1 = thread::spawn(move || {
            drop(token1);
        });

        let th2 = thread::spawn(move || {
            token2.cancel();
        });

        let th3 = thread::spawn(move || {
            drop(child_token);
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}

// Temporarily disabled due to a false positive in loom -
// see https://github.com/tokio-rs/tokio/pull/7644#issuecomment-3328381344
#[ignore]
#[test]
fn cancel_parent_and_child() {
    loom::model(|| {
        let token1 = CancellationToken::new();
        let token2 = token1.clone();
        let child_token = token1.child_token();

        let th1 = thread::spawn(move || {
            drop(token1);
        });

        let th2 = thread::spawn(move || {
            token2.cancel();
        });

        let th3 = thread::spawn(move || {
            child_token.cancel();
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}
