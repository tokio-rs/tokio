use crate::semaphore::{Permit, Semaphore};
use crate::watch::{Receiver, Sender};
use futures_util::{pin_mut, ready};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

/// A barrier enables multiple threads to synchronize the beginning of some computation.
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use std::sync::Arc;
/// use tokio_sync::Barrier;
/// use futures_util::future::join_all;
///
/// let mut handles = Vec::with_capacity(10);
/// let barrier = Arc::new(Barrier::new(10));
/// for _ in 0..10 {
///     let c = barrier.clone();
///     // The same messages will be printed together.
///     // You will NOT see any interleaving.
///     handles.push(async move {
///         println!("before wait");
///         let wr = c.wait().await;
///         println!("after wait");
///         wr
///     });
/// }
/// // Will not resolve until all "before wait" messages have been printed
/// let wrs = join_all(handles).await;
/// // Exactly one barrier will resolve as the "leader"
/// assert_eq!(wrs.into_iter().filter(|wr| wr.is_leader()).count(), 1);
/// # }
/// ```
#[derive(Debug)]
pub struct Barrier {
    semaphore: Semaphore,
    generation: AtomicUsize,
    waker: Sender<usize>,
    wait: Receiver<usize>,
}

struct BarrierWaitFuture<'a> {
    permit: Permit,
    barrier: &'a Barrier,
    generation: usize,
    wait: Receiver<usize>,
}

impl Barrier {
    /// Creates a new barrier that can block a given number of threads.
    ///
    /// A barrier will block `n`-1 threads which call [`wait`] and then wake up
    /// all threads at once when the `n`th thread calls [`wait`].
    pub fn new(n: usize) -> Barrier {
        let (waker, wait) = crate::watch::channel(usize::max_value());
        Barrier {
            semaphore: Semaphore::new(n),
            generation: AtomicUsize::new(0),
            waker,
            wait,
        }
    }

    /// Does nto resolve until all tasks have rendezvoused here.
    ///
    /// Barriers are re-usable after all threads have rendezvoused once, and can
    /// be used continuously.
    ///
    /// A single (arbitrary) future will receive a [`BarrierWaitResult`] that
    /// returns `true` from [`is_leader`] when returning from this function, and
    /// all other threads will receive a result that will return `false` from
    /// [`is_leader`].
    pub async fn wait(&self) -> BarrierWaitResult {
        let generation = self.generation.load(Ordering::Acquire);
        let wait = self.wait.clone();
        BarrierWaitFuture {
            permit: Permit::new(),
            barrier: self,
            generation,
            wait,
        }
        .await
    }
}

impl Future for BarrierWaitFuture<'_> {
    type Output = BarrierWaitResult;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut *self;

        if !this.permit.is_acquired() {
            ready!(this.permit.poll_acquire(cx, &this.barrier.semaphore))
                .expect("semaphore was never closed");
        }

        if this.barrier.semaphore.available_permits() == 0 {
            // we _may_ have to wake everyone up
            if this.barrier.generation.compare_and_swap(
                this.generation,
                this.generation + 1,
                Ordering::AcqRel,
            ) == this.generation
            {
                // yes indeed -- it falls to us
                this.permit.release(&this.barrier.semaphore);
                this.barrier
                    .waker
                    .broadcast(this.generation)
                    .expect("we are still holding a Receiver");
                Poll::Ready(BarrierWaitResult(true))
            } else {
                // someone else is already doing it
                this.permit.release(&this.barrier.semaphore);
                Poll::Ready(BarrierWaitResult(false))
            }
        } else {
            // not everyone has arrived -- we need to wait
            // it's a little awkward to have to re-create this future each time
            loop {
                let generation = this.wait.recv();
                pin_mut!(generation);
                if ready!(generation.as_mut().poll(cx)) == Some(this.generation) {
                    break;
                }
            }

            // the generation has rolled around already!
            this.permit.release(&this.barrier.semaphore);
            Poll::Ready(BarrierWaitResult(false))
        }
    }
}

/// A `BarrierWaitResult` is returned by `wait` when all threads in the `Barrier` have rendezvoused.
#[derive(Debug, Copy, Clone)]
pub struct BarrierWaitResult(bool);

impl BarrierWaitResult {
    /// Returns true if this thread from wait is the "leader thread".
    ///
    /// Only one thread will have `true` returned from their result, all other threads will have
    /// `false` returned.
    pub fn is_leader(&self) -> bool {
        self.0
    }
}
