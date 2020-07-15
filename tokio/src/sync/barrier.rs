use crate::sync::watch;

use std::sync::Mutex;

/// A barrier enables multiple threads to synchronize the beginning of some computation.
///
/// ```
/// # #[tokio::main]
/// # async fn main() {
/// use tokio::sync::Barrier;
///
/// use futures::future::join_all;
/// use std::sync::Arc;
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
    state: Mutex<BarrierState>,
    wait: watch::Receiver<usize>,
    n: usize,
}

#[derive(Debug)]
struct BarrierState {
    waker: watch::Sender<usize>,
    arrived: usize,
    generation: usize,
}

impl Barrier {
    /// Creates a new barrier that can block a given number of threads.
    ///
    /// A barrier will block `n`-1 threads which call [`Barrier::wait`] and then wake up all
    /// threads at once when the `n`th thread calls `wait`.
    pub fn new(mut n: usize) -> Barrier {
        let (waker, wait) = crate::sync::watch::channel(0);

        if n == 0 {
            // if n is 0, it's not clear what behavior the user wants.
            // in std::sync::Barrier, an n of 0 exhibits the same behavior as n == 1, where every
            // .wait() immediately unblocks, so we adopt that here as well.
            n = 1;
        }

        Barrier {
            state: Mutex::new(BarrierState {
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
        // NOTE: we are taking a _synchronous_ lock here.
        // It is okay to do so because the critical section is fast and never yields, so it cannot
        // deadlock even if another future is concurrently holding the lock.
        // It is _desireable_ to do so as synchronous Mutexes are, at least in theory, faster than
        // the asynchronous counter-parts, so we should use them where possible [citation needed].
        // NOTE: the extra scope here is so that the compiler doesn't think `state` is held across
        // a yield point, and thus marks the returned future as !Send.
        let generation = {
            let mut state = self.state.lock().unwrap();
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

            generation
        };

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
    /// Returns `true` if this thread from wait is the "leader thread".
    ///
    /// Only one thread will have `true` returned from their result, all other threads will have
    /// `false` returned.
    pub fn is_leader(&self) -> bool {
        self.0
    }
}
