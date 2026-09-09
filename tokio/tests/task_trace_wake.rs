//! Regression tests for #8434: task-dump capture must not strand
//! notified children in sub-executors.

#![cfg(all(
    tokio_unstable,
    feature = "taskdump",
    target_os = "linux",
    any(
        target_arch = "aarch64",
        target_arch = "x86",
        target_arch = "x86_64",
        target_arch = "s390x"
    )
))]

use std::future::Future;
use std::task::Poll;

use futures::future::{join_all, select_all};
use futures::stream::{self, FuturesUnordered, StreamExt};
use tokio::runtime::dump::{trace_with, Trace};
use tokio::sync::oneshot;
use tokio::task::{yield_now, JoinSet};
use tokio_test::task::{self, Spawn};

fn channels(count: usize) -> (Vec<oneshot::Receiver<usize>>, impl Future<Output = ()>) {
    let (senders, receivers): (Vec<_>, Vec<_>) = (0..count).map(|_| oneshot::channel()).unzip();
    let make_ready = async move {
        for (value, sender) in senders.into_iter().enumerate() {
            sender.send(value).unwrap();
        }
    };
    (receivers, make_ready)
}

async fn assert_progress<F: Future>(task: &mut Spawn<F>) -> F::Output {
    // Yield so Tokio can drain the deferred-wake queue.
    // Poll the parent even without a wake: a parent re-poll alone cannot repair
    // a lost child notification. Bound the loop so regressions fail, not hang.
    for _ in 0..10 {
        yield_now().await;
        if let Poll::Ready(output) = task.poll() {
            return output;
        }
    }
    panic!("capture stranded a ready child despite 10 subsequent parent polls");
}

// Deliver a notification between a normal poll and a capture poll, so tracing
// consumes a child's ready-queue entry in sub-executors that use one. The child's
// own waker must fire again before that sub-executor will poll it normally.
async fn trace_after_wake<F: Future>(future: F, make_ready: impl Future<Output = ()>) -> F::Output {
    let mut task = task::spawn(future);
    assert!(task.poll().is_pending(), "future must initially be pending");
    make_ready.await;
    assert!(task.is_woken(), "completion must notify the parent");

    let mut leaves = 0;
    let result = trace_with(|| task.poll(), |_| leaves += 1);
    assert!(leaves > 0, "capture must reach a Tokio leaf");

    match result {
        Poll::Ready(output) => output,
        Poll::Pending => assert_progress(&mut task).await,
    }
}

async fn assert_capture_defers_wake() {
    let (sender, receiver) = oneshot::channel();
    let mut task = task::spawn(receiver);
    assert!(task.poll().is_pending());
    assert!(!task.is_woken());

    let mut leaves = 0;
    assert!(trace_with(|| task.poll(), |_| leaves += 1).is_pending());
    assert_eq!(leaves, 1);
    assert!(!task.is_woken(), "capture must defer the leaf's wake");

    for _ in 0..10 {
        yield_now().await;
        if task.is_woken() {
            break;
        }
    }
    assert!(task.is_woken(), "capture must eventually wake the leaf");

    // A normal poll registers the waker without scheduling another wake.
    assert!(task.poll().is_pending());
    yield_now().await;
    assert!(
        !task.is_woken(),
        "normal polling must let an idle future rest"
    );

    sender.send(42).unwrap();
    assert!(task.is_woken());
    assert_eq!(task.poll(), Poll::Ready(Ok(42)));
}

#[tokio::test]
async fn capture_defers_wake_current_thread() {
    assert_capture_defers_wake().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn capture_defers_wake_multi_thread() {
    // Run on a worker: multi-threaded block_on does not drive the scheduler.
    tokio::spawn(assert_capture_defers_wake()).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn trace_with_futures_unordered_on_worker() {
    tokio::spawn(async {
        let (receivers, make_ready) = channels(2);
        let future = receivers
            .into_iter()
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>();

        let mut results = trace_after_wake(future, make_ready).await;
        results.sort_unstable_by_key(|result| *result.as_ref().unwrap());
        assert_eq!(results, vec![Ok(0), Ok(1)]);
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn trace_with_futures_unordered() {
    let (receivers, make_ready) = channels(2);
    let future = receivers
        .into_iter()
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>();

    let mut results = trace_after_wake(future, make_ready).await;
    results.sort_unstable_by_key(|result| *result.as_ref().unwrap());
    assert_eq!(results, vec![Ok(0), Ok(1)]);
}

#[tokio::test]
async fn trace_capture_futures_unordered() {
    let (receivers, make_ready) = channels(2);
    let mut task = task::spawn(
        receivers
            .into_iter()
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>(),
    );
    assert!(task.poll().is_pending());
    make_ready.await;
    assert!(task.is_woken());

    let (result, _trace) = Trace::capture(|| task.poll());
    let mut results = match result {
        Poll::Ready(output) => output,
        Poll::Pending => assert_progress(&mut task).await,
    };
    results.sort_unstable_by_key(|result| *result.as_ref().unwrap());
    assert_eq!(results, vec![Ok(0), Ok(1)]);
}

#[tokio::test]
async fn trace_with_join_set() {
    let mut set = JoinSet::new();
    let child = set.spawn(async { 42 });

    // On this current-thread runtime, the child cannot finish before the
    // initial join_next poll. Finish it before the capture poll instead.
    let make_ready = async move {
        for _ in 0..10 {
            yield_now().await;
            if child.is_finished() {
                return;
            }
        }
        panic!("JoinSet child did not finish before capture");
    };

    let result = trace_after_wake(set.join_next(), make_ready).await;
    assert_eq!(result.unwrap().unwrap(), 42);
    assert!(set.is_empty());
}

#[tokio::test]
async fn trace_with_join_all_large() {
    // Large join_all inputs use FuturesOrdered, backed by FuturesUnordered.
    let (receivers, make_ready) = channels(64);
    let results = trace_after_wake(join_all(receivers), make_ready).await;
    assert_eq!(results, (0..64).map(Ok).collect::<Vec<_>>());
}

#[tokio::test]
async fn trace_with_join_all_small() {
    // Control: small join_all inputs poll every unfinished child directly.
    let (receivers, make_ready) = channels(2);
    let results = trace_after_wake(join_all(receivers), make_ready).await;
    assert_eq!(results, vec![Ok(0), Ok(1)]);
}

#[tokio::test]
async fn trace_with_future_select_all() {
    // Control: future::select_all polls its children without a ready queue.
    let (receivers, make_ready) = channels(2);
    let (result, index, remaining) = trace_after_wake(select_all(receivers), make_ready).await;
    assert_eq!(result, Ok(index));
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining.into_iter().next().unwrap().await, Ok(1 - index));
}

#[tokio::test]
async fn trace_with_stream_select_all() {
    // Unlike future::select_all, stream::select_all uses FuturesUnordered.
    let (receivers, make_ready) = channels(2);
    let future = stream::select_all(receivers.into_iter().map(stream::once)).collect::<Vec<_>>();
    let mut results = trace_after_wake(future, make_ready).await;
    results.sort_unstable_by_key(|result| *result.as_ref().unwrap());
    assert_eq!(results, vec![Ok(0), Ok(1)]);
}

#[tokio::test]
async fn futures_unordered_without_capture() {
    // Control: the same notifications suffice if there is no tracing re-poll.
    let (receivers, make_ready) = channels(2);
    let mut task = task::spawn(
        receivers
            .into_iter()
            .collect::<FuturesUnordered<_>>()
            .collect::<Vec<_>>(),
    );
    assert!(task.poll().is_pending());
    make_ready.await;

    let mut results = assert_progress(&mut task).await;
    results.sort_unstable_by_key(|result| *result.as_ref().unwrap());
    assert_eq!(results, vec![Ok(0), Ok(1)]);
}
