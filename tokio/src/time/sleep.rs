use crate::runtime::{scheduler, Timer};
use crate::time::{error::Error, Duration, Instant};
use crate::util::trace;

use pin_project_lite::pin_project;
use std::future::Future;
use std::panic::Location;
use std::pin::Pin;
use std::task::{self, ready, Poll};

/// Waits until `deadline` is reached.
///
/// No work is performed while awaiting on the sleep future to complete. `Sleep`
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// To run something regularly on a schedule, see [`interval`].
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
/// use tokio::time::{sleep_until, Instant, Duration};
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// sleep_until(Instant::now() + Duration::from_millis(100)).await;
/// println!("100 ms have elapsed");
/// # }
/// ```
///
/// See the documentation for the [`Sleep`] type for more examples.
///
/// # Panics
///
/// This function panics if there is no current timer set.
///
/// It can be triggered when [`Builder::enable_time`] or
/// [`Builder::enable_all`] are not included in the builder.
///
/// It can also panic whenever a timer is created outside of a
/// Tokio runtime. That is why `rt.block_on(sleep(...))` will panic,
/// since the function is executed outside of the runtime.
/// Whereas `rt.block_on(async {sleep(...).await})` doesn't panic.
/// And this is because wrapping the function on an async makes it lazy,
/// and so gets executed inside the runtime successfully without
/// panicking.
///
/// [`Sleep`]: struct@crate::time::Sleep
/// [`interval`]: crate::time::interval()
/// [`Builder::enable_time`]: crate::runtime::Builder::enable_time
/// [`Builder::enable_all`]: crate::runtime::Builder::enable_all
// Alias for old name in 0.x
#[cfg_attr(docsrs, doc(alias = "delay_until"))]
#[track_caller]
pub fn sleep_until(deadline: Instant) -> Sleep {
    Sleep::new_timeout(deadline, trace::caller_location())
}

/// Waits until `duration` has elapsed.
///
/// Equivalent to `sleep_until(Instant::now() + duration)`. An asynchronous
/// analog to `std::thread::sleep`.
///
/// No work is performed while awaiting on the sleep future to complete. `Sleep`
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers. The implementation is platform specific,
/// and some platforms (specifically Windows) will provide timers with a
/// larger resolution than 1 ms.
///
/// To run something regularly on a schedule, see [`interval`].
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
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() {
/// sleep(Duration::from_millis(100)).await;
/// println!("100 ms have elapsed");
/// # }
/// ```
///
/// See the documentation for the [`Sleep`] type for more examples.
///
/// # Panics
///
/// This function panics if there is no current timer set.
///
/// It can be triggered when [`Builder::enable_time`] or
/// [`Builder::enable_all`] are not included in the builder.
///
/// It can also panic whenever a timer is created outside of a
/// Tokio runtime. That is why `rt.block_on(sleep(...))` will panic,
/// since the function is executed outside of the runtime.
/// Whereas `rt.block_on(async {sleep(...).await})` doesn't panic.
/// And this is because wrapping the function on an async makes it lazy,
/// and so gets executed inside the runtime successfully without
/// panicking.
///
/// [`Sleep`]: struct@crate::time::Sleep
/// [`interval`]: crate::time::interval()
/// [`Builder::enable_time`]: crate::runtime::Builder::enable_time
/// [`Builder::enable_all`]: crate::runtime::Builder::enable_all
// Alias for old name in 0.x
#[cfg_attr(docsrs, doc(alias = "delay_for"))]
#[cfg_attr(docsrs, doc(alias = "wait"))]
#[track_caller]
pub fn sleep(duration: Duration) -> Sleep {
    let location = trace::caller_location();

    match Instant::now().checked_add(duration) {
        Some(deadline) => Sleep::new_timeout(deadline, location),
        None => Sleep::new_timeout(Instant::far_future(), location),
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
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// sleep(Duration::from_millis(100)).await;
    /// println!("100 ms have elapsed");
    /// # }
    /// ```
    ///
    /// Use with [`select!`]. Pinning the `Sleep` with [`tokio::pin!`] is
    /// necessary when the same `Sleep` is selected on multiple times.
    /// ```no_run
    /// use tokio::time::{self, Duration, Instant};
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let sleep = time::sleep(Duration::from_millis(10));
    /// tokio::pin!(sleep);
    ///
    /// loop {
    ///     tokio::select! {
    ///         () = &mut sleep => {
    ///             println!("timer elapsed");
    ///             sleep.as_mut().reset(Instant::now() + Duration::from_millis(50));
    ///         },
    ///     }
    /// }
    /// # }
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
    #[project(!Unpin)]
    // Alias for old name in 0.2
    #[cfg_attr(docsrs, doc(alias = "Delay"))]
    #[derive(Debug)]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub struct Sleep {
        deadline: Instant,
        driver: scheduler::Handle,
        inner: Inner,
        #[pin]
        timer: Option<Timer>,
    }
}

cfg_trace! {
    #[derive(Debug)]
    struct Inner {
        ctx: trace::AsyncOpTracingCtx,
    }
}

cfg_not_trace! {
    #[derive(Debug)]
    struct Inner {
    }
}

impl Sleep {
    #[cfg_attr(not(all(tokio_unstable, feature = "tracing")), allow(unused_variables))]
    #[track_caller]
    pub(crate) fn new_timeout(
        deadline: Instant,
        location: Option<&'static Location<'static>>,
    ) -> Sleep {
        let handle = scheduler::Handle::current();
        // Panic if the time driver is not enabled (backwards compat)
        _ = handle.driver().time();
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let inner = {
            let location = location.expect("should have location if tracing");
            let resource_span = tracing::trace_span!(
                parent: None,
                "runtime.resource",
                concrete_type = "Sleep",
                kind = "timer",
                loc.file = location.file(),
                loc.line = location.line(),
                loc.col = location.column(),
            );

            let async_op_span = tracing::trace_span!(
                parent: &resource_span,
                "runtime.resource.async_op",
                source = "Sleep::new_timeout",
            );

            let async_op_poll_span =
                tracing::trace_span!(parent: &async_op_span, "runtime.resource.async_op.poll");

            let ctx = trace::AsyncOpTracingCtx {
                async_op_span,
                async_op_poll_span,
                resource_span,
            };

            Inner { ctx }
        };

        #[cfg(not(all(tokio_unstable, feature = "tracing")))]
        let inner = Inner {};

        Sleep {
            deadline,
            driver: handle,
            inner,
            timer: None,
        }
    }

    pub(crate) fn far_future(location: Option<&'static Location<'static>>) -> Sleep {
        Self::new_timeout(Instant::far_future(), location)
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns `true` if `Sleep` has elapsed.
    ///
    /// A `Sleep` instance is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.timer.as_ref().is_some_and(Timer::is_elapsed)
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
    /// See also the top-level examples.
    ///
    /// [`Pin::as_mut`]: fn@std::pin::Pin::as_mut
    pub fn reset(self: Pin<&mut Self>, deadline: Instant) {
        let mut this = self.project();
        *this.deadline = deadline;

        let handle = this.driver;

        #[cfg(all(tokio_unstable, feature = "tracing"))]
        {
            let _resource_enter = this.inner.ctx.resource_span.enter();
            this.inner.ctx.async_op_span =
                tracing::trace_span!("runtime.resource.async_op", source = "Sleep::reset");
            let _async_op_enter = this.inner.ctx.async_op_span.enter();

            this.inner.ctx.async_op_poll_span =
                tracing::trace_span!("runtime.resource.async_op.poll");

            let clock = handle.driver().clock();
            let time_source = handle.driver().time().time_source();
            let now = time_source.now(clock);
            let tick = time_source.deadline_to_tick(deadline);
            tracing::trace!(
                target: "runtime::resource::state_update",
                duration = tick.saturating_sub(now),
                duration.unit = "ms",
                duration.op = "override",
            );
        }

        match this.timer.as_mut().as_pin_mut() {
            Some(timer) => timer.reset(handle.clone(), deadline),
            None => {
                let timer = Timer::new(handle.clone(), deadline);
                this.timer.set(Some(timer));
                this.timer.as_pin_mut().unwrap().init(deadline);
            }
        }
    }

    /// Resets the `Sleep` instance to a new deadline.
    ///
    /// Unlike [`reset`][Self::reset], this __removes__ the internal timer.
    pub(super) fn reset_without_timer(self: Pin<&mut Self>, deadline: Instant) {
        let mut this = self.project();
        *this.deadline = deadline;
        this.timer.set(None);
    }

    fn poll_elapsed(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Result<(), Error>> {
        ready!(crate::trace::trace_leaf());

        // Keep track of task budget
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let coop = ready!(trace_poll_op!(
            "poll_elapsed",
            crate::task::coop::poll_proceed(cx),
        ));

        #[cfg(any(not(tokio_unstable), not(feature = "tracing")))]
        let coop = ready!(crate::task::coop::poll_proceed(cx));

        let mut this = self.project();
        let timer = match this.timer.as_mut().as_pin_mut() {
            Some(timer) => timer,
            None => {
                let handle = this.driver;

                #[cfg(all(tokio_unstable, feature = "tracing"))]
                {
                    let clock = handle.driver().clock();
                    let time_source = handle.driver().time().time_source();
                    let now = time_source.now(clock);
                    let tick = time_source.deadline_to_tick(*this.deadline);
                    tracing::trace!(
                        target: "runtime::resource::state_update",
                        duration = tick.saturating_sub(now),
                        duration.unit = "ms",
                        duration.op = "override",
                    );
                }

                let timer = Timer::new(handle.clone(), *this.deadline);
                this.timer.set(Some(timer));
                let mut timer = this.timer.as_pin_mut().unwrap();
                timer.as_mut().init(*this.deadline);
                timer
            }
        };

        let result = timer.poll_elapsed(cx).map(move |r| {
            coop.made_progress();
            r
        });

        #[cfg(all(tokio_unstable, feature = "tracing"))]
        return trace_poll_op!("poll_elapsed", result);

        #[cfg(any(not(tokio_unstable), not(feature = "tracing")))]
        return result;
    }
}

impl Future for Sleep {
    type Output = ();

    // `poll_elapsed` can return an error in two cases:
    //
    // - AtCapacity: this is a pathological case where far too many
    //   sleep instances have been scheduled.
    // - Shutdown: No timer has been setup, which is a misuse error.
    //
    // Both cases are extremely rare, and pretty accurately fit into
    // "logic errors", so we just panic in this case. A user couldn't
    // really do much better if we passed the error onwards.
    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let _res_span = self.inner.ctx.resource_span.clone().entered();
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let _ao_span = self.inner.ctx.async_op_span.clone().entered();
        #[cfg(all(tokio_unstable, feature = "tracing"))]
        let _ao_poll_span = self.inner.ctx.async_op_poll_span.clone().entered();
        match ready!(self.as_mut().poll_elapsed(cx)) {
            Ok(()) => Poll::Ready(()),
            Err(e) => panic!("timer error: {e}"),
        }
    }
}
