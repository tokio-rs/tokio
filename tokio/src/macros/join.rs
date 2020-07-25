/// Wait on multiple concurrent branches, returning when **all** branches
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
/// [`try_join!`]: macro@try_join
///
/// # Notes
///
/// The supplied futures are stored inline and does not require allocating a
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
#[macro_export]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
macro_rules! join {
    (@ {
        // One `_` for each branch in the `join!` macro. This is not used once
        // normalization is complete.
        ( $($count:tt)* )

        // Normalized join! branches
        $( ( $($skip:tt)* ) $e:expr, )*

    }) => {{
        use $crate::macros::support::{maybe_done, poll_fn, Future, Pin};
        use $crate::macros::support::Poll::{Ready, Pending};

        // Safety: nothing must be moved out of `futures`. This is to satisfy
        // the requirement of `Pin::new_unchecked` called below.
        let mut futures = ( $( maybe_done($e), )* );

        poll_fn(move |cx| {
            let mut is_pending = false;

            $(
                // Extract the future for this branch from the tuple.
                let ( $($skip,)* fut, .. ) = &mut futures;

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
    }};

    // ===== Normalize =====

    (@ { ( $($s:tt)* ) $($t:tt)* } $e:expr, $($r:tt)* ) => {
        $crate::join!(@{ ($($s)* _) $($t)* ($($s)*) $e, } $($r)*)
    };

    // ===== Entry point =====

    ( $($e:expr),* $(,)?) => {
        $crate::join!(@{ () } $($e,)*)
    };
}
