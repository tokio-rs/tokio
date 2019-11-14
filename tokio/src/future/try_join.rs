use crate::future::{maybe_done, MaybeDone};

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pub(crate) fn try_join3<T1, F1, T2, F2, T3, F3, E>(
    future1: F1,
    future2: F2,
    future3: F3,
) -> TryJoin3<F1, F2, F3>
where
    F1: Future<Output = Result<T1, E>>,
    F2: Future<Output = Result<T2, E>>,
    F3: Future<Output = Result<T3, E>>,
{
    TryJoin3 {
        future1: maybe_done(future1),
        future2: maybe_done(future2),
        future3: maybe_done(future3),
    }
}

pub(crate) struct TryJoin3<F1, F2, F3>
where
    F1: Future,
    F2: Future,
    F3: Future,
{
    future1: MaybeDone<F1>,
    future2: MaybeDone<F2>,
    future3: MaybeDone<F3>,
}

impl<T1, F1, T2, F2, T3, F3, E> Future for TryJoin3<F1, F2, F3>
where
    F1: Future<Output = Result<T1, E>>,
    F2: Future<Output = Result<T2, E>>,
    F3: Future<Output = Result<T3, E>>,
{
    type Output = Result<(T1, T2, T3), E>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut all_done = true;

        // Safety: the fn takes `Pin`, we don't move any data out of `self`.
        unsafe {
            let me = self.get_unchecked_mut();

            if Pin::new_unchecked(&mut me.future1).poll(cx).is_pending() {
                all_done = false;
            } else if Pin::new_unchecked(&mut me.future1)
                .output_mut()
                .unwrap()
                .is_err()
            {
                return Poll::Ready(Err(Pin::new_unchecked(&mut me.future1)
                    .take_output()
                    .unwrap()
                    .err()
                    .unwrap()));
            }

            if Pin::new_unchecked(&mut me.future2).poll(cx).is_pending() {
                all_done = false;
            } else if Pin::new_unchecked(&mut me.future2)
                .output_mut()
                .unwrap()
                .is_err()
            {
                return Poll::Ready(Err(Pin::new_unchecked(&mut me.future2)
                    .take_output()
                    .unwrap()
                    .err()
                    .unwrap()));
            }

            if Pin::new_unchecked(&mut me.future3).poll(cx).is_pending() {
                all_done = false;
            } else if Pin::new_unchecked(&mut me.future3)
                .output_mut()
                .unwrap()
                .is_err()
            {
                return Poll::Ready(Err(Pin::new_unchecked(&mut me.future3)
                    .take_output()
                    .unwrap()
                    .err()
                    .unwrap()));
            }

            if all_done {
                Poll::Ready(Ok((
                    Pin::new_unchecked(&mut me.future1)
                        .take_output()
                        .unwrap()
                        .ok()
                        .unwrap(),
                    Pin::new_unchecked(&mut me.future2)
                        .take_output()
                        .unwrap()
                        .ok()
                        .unwrap(),
                    Pin::new_unchecked(&mut me.future3)
                        .take_output()
                        .unwrap()
                        .ok()
                        .unwrap(),
                )))
            } else {
                Poll::Pending
            }
        }
    }
}
