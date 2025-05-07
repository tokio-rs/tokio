macro_rules! doc {
    ($join:item) => {
        /// Waits on multiple concurrent branches, returning when **all** branches
        /// complete.
        ///
        /// The `join!` macro must be used inside of async functions, closures, and
        /// blocks.
        ///
        /// The `join!` macro takes a list of async expressions and evaluates them
        /// concurrently on the same task. Each async expression evaluates to a future
        /// and the futures from each expression are multiplexed on the current task.
        ///
        /// When working with async expressions returning `Result`, `join!` will wait
        /// for **all** branches complete regardless if any complete with `Err`. Use
        /// [`try_join!`] to return early when `Err` is encountered.
        ///
        /// [`try_join!`]: crate::try_join
        ///
        /// # Notes
        ///
        /// The supplied futures are stored inline and do not require allocating a
        /// `Vec`.
        ///
        /// ### Runtime characteristics
        ///
        /// By running all async expressions on the current task, the expressions are
        /// able to run **concurrently** but not in **parallel**. This means all
        /// expressions are run on the same thread and if one branch blocks the thread,
        /// all other expressions will be unable to continue. If parallelism is
        /// required, spawn each async expression using [`tokio::spawn`] and pass the
        /// join handle to `join!`.
        ///
        /// [`tokio::spawn`]: crate::spawn
        ///
        /// # Fairness
        ///
        /// By default, `join!`'s generated future rotates which contained
        /// future is polled first whenever it is woken.
        ///
        /// This behavior can be overridden by adding `biased;` to the beginning of the
        /// macro usage. See the examples for details. This will cause `join` to poll
        /// the futures in the order they appear from top to bottom.
        ///
        /// You may want this if your futures may interact in a way where known polling order is significant.
        ///
        /// But there is an important caveat to this mode. It becomes your responsibility
        /// to ensure that the polling order of your futures is fair. If for example you
        /// are joining a stream and a shutdown future, and the stream has a
        /// huge volume of messages that takes a long time to finish processing per poll, you should
        /// place the shutdown future earlier in the `join!` list to ensure that it is
        /// always polled, and will not be delayed due to the stream future taking a long time to return
        /// `Poll::Pending`.
        ///
        /// # Examples
        ///
        /// Basic join with two branches
        ///
        /// ```
        /// async fn do_stuff_async() {
        ///     // async work
        /// }
        ///
        /// async fn more_async_work() {
        ///     // more here
        /// }
        ///
        /// #[tokio::main]
        /// async fn main() {
        ///     let (first, second) = tokio::join!(
        ///         do_stuff_async(),
        ///         more_async_work());
        ///
        ///     // do something with the values
        /// }
        /// ```
        ///
        ///  Using the `biased;` mode to control polling order.
        ///
        /// ```
        /// async fn do_stuff_async() {
        ///     // async work
        /// }
        ///
        /// async fn more_async_work() {
        ///     // more here
        /// }
        ///
        /// #[tokio::main]
        /// async fn main() {
        ///     let (first, second) = tokio::join!(
        ///         // do_stuff_async() will always be polled first when woken
        ///         biased;
        ///         do_stuff_async(),
        ///         more_async_work());
        ///
        ///     // do something with the values
        /// }
        /// ```

        #[macro_export]
        #[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
        $join
    };
}

#[cfg(doc)]
doc! {macro_rules! join {
    ($(biased;)? $($future:expr),*) => { unimplemented!() }
}}

#[cfg(not(doc))]
doc! {macro_rules! join {
    (@ {
        // Whether to rotate which future is polled first every poll,
        // by incrementing a skip counter
        rotate_poll_order=$rotate_poll_order:literal;

        // One `_` for each branch in the `join!` macro. This is not used once
        // normalization is complete.
        ( $($count:tt)* )

        // The expression `0+1+1+ ... +1` equal to the number of branches.
        ( $($total:tt)* )

        // Normalized join! branches
        $( ( $($skip:tt)* ) $e:expr, )*

    }) => {{
        use $crate::macros::support::{maybe_done, poll_fn, Future, Pin};
        use $crate::macros::support::Poll::{Ready, Pending};

        // Safety: nothing must be moved out of `futures`. This is to satisfy
        // the requirement of `Pin::new_unchecked` called below.
        //
        // We can't use the `pin!` macro for this because `futures` is a tuple
        // and the standard library provides no way to pin-project to the fields
        // of a tuple.
        let mut futures = ( $( maybe_done($e), )* );

        // This assignment makes sure that the `poll_fn` closure only has a
        // reference to the futures, instead of taking ownership of them. This
        // mitigates the issue described in
        // <https://internals.rust-lang.org/t/surprising-soundness-trouble-around-pollfn/17484>
        let mut futures = &mut futures;

        // Each time the future created by poll_fn is polled,
        // if not running in biased mode,
        // a different future will be polled first
        // to ensure every future passed to join! gets a chance to make progress even if
        // one of the futures consumes the whole budget.
        //
        // This is number of futures that will be skipped in the first loop
        // iteration the next time poll.
        //
        // If running in biased mode, this variable will be optimized out since we don't pass
        // it to the poll_fn.
        let mut skip_next_time: u32 = 0;

        match $rotate_poll_order {
            true => poll_fn(move |cx| {
                const COUNT: u32 = $($total)*;
                let mut is_pending = false;
                let mut to_run = COUNT;

                // The number of futures that will be skipped in the first loop iteration.
                let mut skip = skip_next_time;
                // Upkeep for next poll, rotate first polled future
                skip_next_time = if skip + 1 == COUNT { 0 } else { skip + 1 };

                // This loop runs twice and the first `skip` futures
                // are not polled in the first iteration.
                loop {
                    $(
                        if skip == 0 {
                            if to_run == 0 {
                                // Every future has been polled
                                break;
                            }
                            to_run -= 1;

                            // Extract the future for this branch from the tuple.
                            let ( $($skip,)* fut, .. ) = &mut *futures;

                            // Safety: future is stored on the stack above
                            // and never moved.
                            let mut fut = unsafe { Pin::new_unchecked(fut) };

                            // Try polling
                            if fut.poll(cx).is_pending() {
                                is_pending = true;
                            }
                        } else {
                            // Future skipped, one less future to skip in the next iteration
                            skip -= 1;
                        }
                    )*
                    }

                    if is_pending {
                        Pending
                    } else {
                        Ready(($({
                            // Extract the future for this branch from the tuple.
                            let ( $($skip,)* fut, .. ) = &mut futures;

                            // Safety: future is stored on the stack above
                            // and never moved.
                            let mut fut = unsafe { Pin::new_unchecked(fut) };

                            fut.take_output().expect("expected completed future")
                        },)*))
                    }
            }).await,
            // don't rotate the poll order so no skipping
            false => poll_fn(move |cx| {
                const COUNT: u32 = $($total)*;
                let mut is_pending = false;
                let mut to_run = COUNT;

                // no loop since we don't skip the first time through
                $(
                    // Extract the future for this branch from the tuple.
                    let ( $($skip,)* fut, .. ) = &mut *futures;

                    // Safety: future is stored on the stack above
                    // and never moved.
                    let mut fut = unsafe { Pin::new_unchecked(fut) };

                    // Try polling
                    if fut.poll(cx).is_pending() {
                        is_pending = true;
                    }
                )*

                if is_pending {
                    Pending
                } else {
                    Ready(($({
                        // Extract the future for this branch from the tuple.
                        let ( $($skip,)* fut, .. ) = &mut futures;

                        // Safety: future is stored on the stack above
                        // and never moved.
                        let mut fut = unsafe { Pin::new_unchecked(fut) };

                        fut.take_output().expect("expected completed future")
                    },)*))
                }
            }).await
        }
    }};

    // ===== Normalize =====

    (@ { rotate_poll_order=$rotate_poll_order:literal; ( $($s:tt)* ) ( $($n:tt)* ) $($t:tt)* } $e:expr, $($r:tt)* ) => {
        $crate::join!(@{ rotate_poll_order=$rotate_poll_order; ($($s)* _) ($($n)* + 1) $($t)* ($($s)*) $e, } $($r)*)
    };

    // ===== Entry point =====
    ( biased; $($e:expr),+ $(,)?) => {
        $crate::join!(@{ rotate_poll_order=false; () (0) } $($e,)*)
    };

    ( $($e:expr),+ $(,)?) => {
        $crate::join!(@{ rotate_poll_order=true; () (0) } $($e,)*)
    };

    (biased;) => { async {}.await };

    () => { async {}.await }
}}
