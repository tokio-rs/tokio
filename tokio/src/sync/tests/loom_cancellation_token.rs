use crate::sync::CancellationToken;

use loom::{
    future::block_on,
    thread,
};
use tokio_test::assert_ok;

#[test]
fn cancel_token() {
    loom::model(|| {
        let token = CancellationToken::new();
        let token1 = token.clone();

        let th1 = thread::spawn(move || {
            block_on(async {
                token1.wait_for_cancellation().await;
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
                token1.wait_for_cancellation().await;
            });
        });

        let th2 = thread::spawn(move || {
            token2.cancel();
        });

        let th3 = thread::spawn(move || {
            block_on(async {
                child_token.wait_for_cancellation().await;
            });
        });

        assert_ok!(th1.join());
        assert_ok!(th2.join());
        assert_ok!(th3.join());
    });
}

#[test]
fn drop_token() {
    loom::model(|| {
        let token = CancellationToken::new();
        let token1 = token.clone();
        let token2 = token.clone();
        let child_token = token.child_token();

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
