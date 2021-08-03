#![warn(rust_2018_idioms)]

use tokio::pin;
use tokio_util::sync::CancellationToken;

use core::future::Future;
use core::task::{Context, Poll};
use futures_test::task::new_count_waker;

#[test]
fn cancel_token() {
    let (waker, wake_counter) = new_count_waker();
    let token = CancellationToken::new();
    assert!(!token.is_cancelled());

    let wait_fut = token.cancelled();
    pin!(wait_fut);

    assert_eq!(
        Poll::Pending,
        wait_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(wake_counter, 0);

    let wait_fut_2 = token.cancelled();
    pin!(wait_fut_2);

    token.cancel();
    assert_eq!(wake_counter, 1);
    assert!(token.is_cancelled());

    assert_eq!(
        Poll::Ready(()),
        wait_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Ready(()),
        wait_fut_2.as_mut().poll(&mut Context::from_waker(&waker))
    );
}

#[test]
fn cancel_child_token_through_parent() {
    let (waker, wake_counter) = new_count_waker();
    let token = CancellationToken::new();

    let child_token = token.child_token();
    assert!(!child_token.is_cancelled());

    let child_fut = child_token.cancelled();
    pin!(child_fut);
    let parent_fut = token.cancelled();
    pin!(parent_fut);

    assert_eq!(
        Poll::Pending,
        child_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Pending,
        parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(wake_counter, 0);

    token.cancel();
    assert_eq!(wake_counter, 2);
    assert!(token.is_cancelled());
    assert!(child_token.is_cancelled());

    assert_eq!(
        Poll::Ready(()),
        child_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Ready(()),
        parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
}

#[test]
fn cancel_child_token_without_parent() {
    let (waker, wake_counter) = new_count_waker();
    let token = CancellationToken::new();

    let child_token_1 = token.child_token();

    let child_fut = child_token_1.cancelled();
    pin!(child_fut);
    let parent_fut = token.cancelled();
    pin!(parent_fut);

    assert_eq!(
        Poll::Pending,
        child_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Pending,
        parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(wake_counter, 0);

    child_token_1.cancel();
    assert_eq!(wake_counter, 1);
    assert!(!token.is_cancelled());
    assert!(child_token_1.is_cancelled());

    assert_eq!(
        Poll::Ready(()),
        child_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Pending,
        parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );

    let child_token_2 = token.child_token();
    let child_fut_2 = child_token_2.cancelled();
    pin!(child_fut_2);

    assert_eq!(
        Poll::Pending,
        child_fut_2.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Pending,
        parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );

    token.cancel();
    assert_eq!(wake_counter, 3);
    assert!(token.is_cancelled());
    assert!(child_token_2.is_cancelled());

    assert_eq!(
        Poll::Ready(()),
        child_fut_2.as_mut().poll(&mut Context::from_waker(&waker))
    );
    assert_eq!(
        Poll::Ready(()),
        parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
    );
}

#[test]
fn create_child_token_after_parent_was_cancelled() {
    for drop_child_first in [true, false].iter().cloned() {
        let (waker, wake_counter) = new_count_waker();
        let token = CancellationToken::new();
        token.cancel();

        let child_token = token.child_token();
        assert!(child_token.is_cancelled());

        {
            let child_fut = child_token.cancelled();
            pin!(child_fut);
            let parent_fut = token.cancelled();
            pin!(parent_fut);

            assert_eq!(
                Poll::Ready(()),
                child_fut.as_mut().poll(&mut Context::from_waker(&waker))
            );
            assert_eq!(
                Poll::Ready(()),
                parent_fut.as_mut().poll(&mut Context::from_waker(&waker))
            );
            assert_eq!(wake_counter, 0);

            drop(child_fut);
            drop(parent_fut);
        }

        if drop_child_first {
            drop(child_token);
            drop(token);
        } else {
            drop(token);
            drop(child_token);
        }
    }
}

#[test]
fn drop_multiple_child_tokens() {
    for drop_first_child_first in &[true, false] {
        let token = CancellationToken::new();
        let mut child_tokens = [None, None, None];
        for child in &mut child_tokens {
            *child = Some(token.child_token());
        }

        assert!(!token.is_cancelled());
        assert!(!child_tokens[0].as_ref().unwrap().is_cancelled());

        for i in 0..child_tokens.len() {
            if *drop_first_child_first {
                child_tokens[i] = None;
            } else {
                child_tokens[child_tokens.len() - 1 - i] = None;
            }
            assert!(!token.is_cancelled());
        }

        drop(token);
    }
}

#[test]
fn drop_parent_before_child_tokens() {
    let token = CancellationToken::new();
    let child1 = token.child_token();
    let child2 = token.child_token();

    drop(token);
    assert!(!child1.is_cancelled());

    drop(child1);
    drop(child2);
}
