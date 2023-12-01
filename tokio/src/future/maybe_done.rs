//! Definition of the [`MaybeDone`] combinator.

use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

pin_project! {
    /// A future that may have completed.
    #[derive(Debug)]
    #[project = MaybeDoneProj]
    #[project_replace = MaybeDoneProjReplace]
    pub enum MaybeDone<Fut: Future> {
        /// A not-yet-completed future.
        Future { #[pin] future: Fut },
        /// The output of the completed future.
        Done { output: Fut::Output },
        /// The empty variant after the result of a [`MaybeDone`] has been
        /// taken using the [`take_output`](MaybeDone::take_output) method.
        Gone,
    }
}

/// Wraps a future into a `MaybeDone`.
pub fn maybe_done<Fut: Future>(future: Fut) -> MaybeDone<Fut> {
    MaybeDone::Future { future }
}

impl<Fut: Future> MaybeDone<Fut> {
    /// Returns an [`Option`] containing a mutable reference to the output of the future.
    /// The output of this method will be [`Some`] if and only if the inner
    /// future has been completed and [`take_output`](MaybeDone::take_output)
    /// has not yet been called.
    pub fn output_mut(self: Pin<&mut Self>) -> Option<&mut Fut::Output> {
        match self.project() {
            MaybeDoneProj::Done { output } => Some(output),
            _ => None,
        }
    }

    /// Attempts to take the output of a `MaybeDone` without driving it
    /// towards completion.
    #[inline]
    pub fn take_output(self: Pin<&mut Self>) -> Option<Fut::Output> {
        match *self {
            MaybeDone::Done { .. } => {}
            MaybeDone::Future { .. } | MaybeDone::Gone => return None,
        };
        if let MaybeDoneProjReplace::Done { output } = self.project_replace(MaybeDone::Gone) {
            Some(output)
        } else {
            unreachable!()
        }
    }
}

impl<Fut: Future> Future for MaybeDone<Fut> {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let output = match self.as_mut().project() {
            MaybeDoneProj::Future { future } => ready!(future.poll(cx)),
            MaybeDoneProj::Done { .. } => return Poll::Ready(()),
            MaybeDoneProj::Gone => panic!("MaybeDone polled after value taken"),
        };
        self.set(MaybeDone::Done { output });
        Poll::Ready(())
    }
}
