use crate::time::driver::{Handle, TimerEntry};
use crate::time::{error::Error, Duration, Instant};

use pin_project_lite::pin_project;
use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};

/// Waits until `deadline` is reached.
///
/// No work is performed while awaiting on the sleep future to complete. `Sleep`
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// # Cancellation
///
/// Canceling a sleep instance is done by dropping the returned future. No additional
/// cleanup work is required.
// Alias for old name in 0.x
#[cfg_attr(docsrs, doc(alias = "delay_until"))]
pub fn sleep_until(deadline: Instant) -> Sleep {
    Sleep::new_timeout(deadline)
}

/// Waits until `duration` has elapsed.
///
/// Equivalent to `sleep_until(Instant::now() + duration)`. An asynchronous
/// analog to `std::thread::sleep`.
///
/// No work is performed while awaiting on the sleep future to complete. `Sleep`
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// To run something regularly on a schedule, see [`interval`].
///
/// The maximum duration for a sleep is 68719476734 milliseconds (approximately 2.2 years).
///
/// # Cancellation
///
/// Canceling a sleep instance is done by dropping the returned future. No additional
/// cleanup work is required.
///
/// # Examples
///
/// Wait 100ms and print "100 ms have elapsed".
///
/// ```
/// use tokio::time::{sleep, Duration};
///
/// #[tokio::main]
/// async fn main() {
///     sleep(Duration::from_millis(100)).await;
///     println!("100 ms have elapsed");
/// }
/// ```
///
/// [`interval`]: crate::time::interval()
// Alias for old name in 0.x
#[cfg_attr(docsrs, doc(alias = "delay_for"))]
#[cfg_attr(docsrs, doc(alias = "wait"))]
pub fn sleep(duration: Duration) -> Sleep {
    match Instant::now().checked_add(duration) {
        Some(deadline) => sleep_until(deadline),
        None => sleep_until(Instant::far_future()),
    }
}

pin_project! {
    /// Future returned by [`sleep`](sleep) and [`sleep_until`](sleep_until).
    ///
    /// This type does not implement the `Unpin` trait, which means that if you
    /// use it with [`select!`] or by calling `poll`, you have to pin it first.
    /// If you use it with `.await`, this does not apply.
    ///
    /// # Examples
    ///
    /// Wait 100ms and print "100 ms have elapsed".
    ///
    /// ```
    /// use tokio::time::{sleep, Duration};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     sleep(Duration::from_millis(100)).await;
    ///     println!("100 ms have elapsed");
    /// }
    /// ```
    ///
    /// Use with [`select!`]. Pinning the `Sleep` with [`tokio::pin!`] is
    /// necessary when the same `Sleep` is selected on multiple times.
    /// ```no_run
    /// use tokio::time::{self, Duration, Instant};
    ///
    /// #[tokio::main]
    /// async fn main() {
    ///     let sleep = time::sleep(Duration::from_millis(10));
    ///     tokio::pin!(sleep);
    ///
    ///     loop {
    ///         tokio::select! {
    ///             () = &mut sleep => {
    ///                 println!("timer elapsed");
    ///                 sleep.as_mut().reset(Instant::now() + Duration::from_millis(50));
    ///             },
    ///         }
    ///     }
    /// }
    /// ```
    /// Use in a struct with boxing. By pinning the `Sleep` with a `Box`, the
    /// `HasSleep` struct implements `Unpin`, even though `Sleep` does not.
    /// ```
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    /// use tokio::time::Sleep;
    ///
    /// struct HasSleep {
    ///     sleep: Pin<Box<Sleep>>,
    /// }
    ///
    /// impl Future for HasSleep {
    ///     type Output = ();
    ///
    ///     fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
    ///         self.sleep.as_mut().poll(cx)
    ///     }
    /// }
    /// ```
    /// Use in a struct with pin projection. This method avoids the `Box`, but
    /// the `HasSleep` struct will not be `Unpin` as a consequence.
    /// ```
    /// use std::future::Future;
    /// use std::pin::Pin;
    /// use std::task::{Context, Poll};
    /// use tokio::time::Sleep;
    /// use pin_project_lite::pin_project;
    ///
    /// pin_project! {
    ///     struct HasSleep {
    ///         #[pin]
    ///         sleep: Sleep,
    ///     }
    /// }
    ///
    /// impl Future for HasSleep {
    ///     type Output = ();
    ///
    ///     fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
    ///         self.project().sleep.poll(cx)
    ///     }
    /// }
    /// ```
    ///
    /// [`select!`]: ../macro.select.html
    /// [`tokio::pin!`]: ../macro.pin.html
    // Alias for old name in 0.2
    #[cfg_attr(docsrs, doc(alias = "Delay"))]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Sleep {
        deadline: Instant,

        // The link between the `Sleep` instance and the timer that drives it.
        #[pin]
        entry: TimerEntry,
    }
}

impl Sleep {
    pub(crate) fn new_timeout(deadline: Instant) -> Sleep {
        let handle = Handle::current();
        let entry = TimerEntry::new(&handle, deadline);

        Sleep { deadline, entry }
    }

    pub(crate) fn far_future() -> Sleep {
        Self::new_timeout(Instant::far_future())
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns `true` if `Sleep` has elapsed.
    ///
    /// A `Sleep` instance is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.entry.is_elapsed()
    }

    /// Resets the `Sleep` instance to a new deadline.
    ///
    /// Calling this function allows changing the instant at which the `Sleep`
    /// future completes without having to create new associated state.
    ///
    /// This function can be called both before and after the future has
    /// completed.
    ///
    /// To call this method, you will usually combine the call with
    /// [`Pin::as_mut`], which lets you call the method without consuming the
    /// `Sleep` itself.
    ///
    /// # Example
    ///
    /// ```
    /// use tokio::time::{Duration, Instant};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let sleep = tokio::time::sleep(Duration::from_millis(10));
    /// tokio::pin!(sleep);
    ///
    /// sleep.as_mut().reset(Instant::now() + Duration::from_millis(20));
    /// # }
    /// ```
    ///
    /// [`Pin::as_mut`]: fn@std::pin::Pin::as_mut
    pub fn reset(self: Pin<&mut Self>, deadline: Instant) {
        let me = self.project();
        me.entry.reset(deadline);
        *me.deadline = deadline;
    }

    fn poll_elapsed(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        let me = self.project();

        // Keep track of task budget
        let coop = ready!(crate::coop::poll_proceed(cx));

        me.entry.poll_elapsed(cx).map(move |r| {
            coop.made_progress();
            r
        })
    }
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        // `poll_elapsed` can return an error in two cases:
        //
        // - AtCapacity: this is a pathological case where far too many
        //   sleep instances have been scheduled.
        // - Shutdown: No timer has been setup, which is a mis-use error.
        //
        // Both cases are extremely rare, and pretty accurately fit into
        // "logic errors", so we just panic in this case. A user couldn't
        // really do much better if we passed the error onwards.
        match ready!(self.as_mut().poll_elapsed(cx)) {
            Ok(()) => Poll::Ready(()),
            Err(e) => panic!("timer error: {}", e),
        }
    }
}
