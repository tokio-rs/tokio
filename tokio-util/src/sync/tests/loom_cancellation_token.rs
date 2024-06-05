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
fn run_until_cancelled_completes() {
    loom::model(|| {
        block_on(async {
            let token = CancellationToken::new();

            let fut = async {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                42
            };

            if let Some(res) = token.run_until_cancelled(fut).await {
                assert_eq!(res, 42);
            } else {
                panic!("Should not happen since we are not cancelling the token");
            }
        });
    });
}

#[test]
fn run_until_cancelled_with_cancel() {
    loom::model(|| {
        block_on(async {
            let token = CancellationToken::new();
            let token1 = token.clone();

            let th1 = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                token1.cancel();
            });

            if let None = token.run_until_cancelled(std::future::pending).await {
                assert!(true);
            } else {
                panic!("Should not happen since token got cancelled before future could finish");
            }

            assert_ok!(th1.join());
        });
    });
}
