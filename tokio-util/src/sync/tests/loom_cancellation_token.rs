use crate::sync::CancellationToken;

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

#[test]
fn is_cancelled_release_acquire() {
    loom::model(|| {
        let data = loom::sync::Arc::new(loom::sync::atomic::AtomicUsize::new(0));
        let parent = CancellationToken::new();
        let child = parent.child_token();

        let data_clone = data.clone();
        let parent_clone = parent.clone();
        let t1 = loom::thread::spawn(move || {
            data_clone.store(42, loom::sync::atomic::Ordering::Relaxed);
            parent_clone.cancel();
        });

        if child.is_cancelled() {
            assert_eq!(data.load(loom::sync::atomic::Ordering::Relaxed), 42);
        }

        t1.join().unwrap();
        assert!(parent.is_cancelled());
        assert!(child.is_cancelled());
        assert_eq!(data.load(loom::sync::atomic::Ordering::Relaxed), 42);
    });
}
