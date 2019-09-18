use crate::watch::{Receiver, Sender};
use crate::Lock;

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
    state: Lock<BarrierState>,
    wait: Receiver<usize>,
    n: usize,
}

#[derive(Debug)]
struct BarrierState {
    waker: Sender<usize>,
    arrived: usize,
    generation: usize,
}

impl Barrier {
    /// Creates a new barrier that can block a given number of threads.
    ///
    /// A barrier will block `n`-1 threads which call [`Barrier::wait`] and then wake up all
    /// threads at once when the `n`th thread calls `wait`.
    pub fn new(n: usize) -> Barrier {
        let (waker, wait) = crate::watch::channel(0);
        Barrier {
            state: Lock::new(BarrierState {
                waker,
                arrived: 0,
                generation: 1,
            }),
            n,
            wait,
        }
    }

    /// Does not resolve until all tasks have rendezvoused here.
    ///
    /// Barriers are re-usable after all threads have rendezvoused once, and can
    /// be used continuously.
    ///
    /// A single (arbitrary) future will receive a [`BarrierWaitResult`] that returns `true` from
    /// [`BarrierWaitResult::is_leader`] when returning from this function, and all other threads
    /// will receive a result that will return `false` from `is_leader`.
    pub async fn wait(&self) -> BarrierWaitResult {
        let mut lock = self.state.clone();
        let mut state = lock.lock().await;
        let generation = state.generation;
        state.arrived += 1;
        if state.arrived == self.n {
            // we are the leader for this generation
            // wake everyone, increment the generation, and return
            state
                .waker
                .broadcast(state.generation)
                .expect("there is at least one receiver");
            state.arrived = 0;
            state.generation += 1;
            return BarrierWaitResult(true);
        }

        drop(state);

        // we're going to have to wait for the last of the generation to arrive
        let mut wait = self.wait.clone();

        loop {
            // note that the first time through the loop, this _will_ yield a generation
            // immediately, since we cloned a receiver that has never seen any values.
            if wait.recv().await.expect("sender hasn't been closed") >= generation {
                break;
            }
        }

        BarrierWaitResult(false)
    }
}

/// A `BarrierWaitResult` is returned by `wait` when all threads in the `Barrier` have rendezvoused.
#[derive(Debug, Clone)]
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
