#![warn(rust_2018_idioms)]

use tokio::sync::oneshot;
use tokio::task::yield_now;
use tokio::time::Duration;
use tokio_test::{assert_pending, assert_ready, task};
use tokio_util::task::JoinQueue;

#[tokio::test]
async fn test_join_queue_no_spurious_wakeups() {
    let (tx, rx) = oneshot::channel::<()>();
    let mut join_queue = JoinQueue::new();
    join_queue.spawn(async move {
        let _ = rx.await;
        42
    });

    let mut join_next = task::spawn(join_queue.join_next());

    assert_pending!(join_next.poll());

    assert!(!join_next.is_woken());

    let _ = tx.send(());
    yield_now().await;

    assert!(join_next.is_woken());

    let output = assert_ready!(join_next.poll());
    assert_eq!(output.unwrap().unwrap(), 42);
}

#[tokio::test]
async fn test_join_queue_abort_on_drop() {
    let mut queue = JoinQueue::new();

    let mut recvs = Vec::new();

    for _ in 0..16 {
        let (send, recv) = oneshot::channel::<()>();
        recvs.push(recv);

        queue.spawn(async move {
            // This task will never complete on its own.
            futures::future::pending::<()>().await;
            drop(send);
        });
    }

    drop(queue);

    for recv in recvs {
        // The task is aborted soon and we will receive an error.
        assert!(recv.await.is_err());
    }
}

#[tokio::test]
async fn test_join_queue_alternating() {
    let mut queue = JoinQueue::new();

    assert_eq!(queue.len(), 0);
    queue.spawn(async {});
    assert_eq!(queue.len(), 1);
    queue.spawn(async {});
    assert_eq!(queue.len(), 2);

    for _ in 0..16 {
        let res = queue.join_next().await.unwrap();
        assert!(res.is_ok());
        assert_eq!(queue.len(), 1);
        queue.spawn(async {});
        assert_eq!(queue.len(), 2);
    }
}

#[tokio::test(start_paused = true)]
async fn test_join_queue_abort_all() {
    let mut queue: JoinQueue<()> = JoinQueue::new();

    for _ in 0..5 {
        queue.spawn(futures::future::pending());
    }
    for _ in 0..5 {
        queue.spawn(async {
            tokio::time::sleep(Duration::from_secs(1)).await;
        });
    }

    // The join queue will now have 5 pending tasks and 5 ready tasks.
    tokio::time::sleep(Duration::from_secs(2)).await;

    queue.abort_all();
    assert_eq!(queue.len(), 10);

    let mut count = 0;
    while let Some(res) = queue.join_next().await {
        if count < 5 {
            assert!(res.unwrap_err().is_cancelled());
        } else {
            assert!(res.is_ok());
        }
        count += 1;
    }
    assert_eq!(count, 10);
    assert!(queue.is_empty());
}

#[tokio::test]
async fn test_join_queue_join_all() {
    let mut queue = JoinQueue::new();
    let mut senders = Vec::new();
    for i in 0..5 {
        let (tx, rx) = oneshot::channel::<()>();
        senders.push(tx);
        queue.spawn(async move {
            let _ = rx.await;
            i
        });
    }
    // Complete all tasks in reverse order
    while let Some(tx) = senders.pop() {
        let _ = tx.send(());
    }
    let results = queue.join_all().await;
    assert_eq!(results, vec![0, 1, 2, 3, 4]);
}

#[tokio::test]
async fn test_join_queue_shutdown() {
    let mut queue = JoinQueue::new();
    let mut senders = Vec::new();

    for _ in 0..5 {
        let (tx, rx) = oneshot::channel::<()>();
        senders.push(tx);
        queue.spawn(async move {
            let _ = rx.await;
        });
    }

    queue.shutdown().await;
    assert!(queue.is_empty());
    while let Some(tx) = senders.pop() {
        assert!(tx.is_closed());
    }
}

#[tokio::test]
async fn test_join_queue_with_manual_abort() {
    let mut queue = JoinQueue::new();
    let mut num_canceled = 0;
    let mut num_completed = 0;
    let mut senders = Vec::new();
    for i in 0..16 {
        let (tx, rx) = oneshot::channel::<()>();
        senders.push(tx);
        let abort = queue.spawn(async move {
            let _ = rx.await;
            i
        });

        if i % 2 != 0 {
            // abort odd-numbered tasks.
            abort.abort();
        }
    }
    // Complete all tasks in reverse order
    while let Some(tx) = senders.pop() {
        let _ = tx.send(());
    }
    while let Some(res) = queue.join_next().await {
        match res {
            Ok(res) => {
                assert_eq!(res, num_completed * 2);
                num_completed += 1;
            }
            Err(e) => {
                assert!(e.is_cancelled());
                num_canceled += 1;
            }
        }
    }

    assert_eq!(num_canceled, 8);
    assert_eq!(num_completed, 8);
}

#[tokio::test]
async fn test_join_queue_join_next_with_id() {
    const TASK_NUM: u32 = 1000;

    let (send, recv) = tokio::sync::watch::channel(());

    let mut set = JoinQueue::new();
    let mut spawned = Vec::with_capacity(TASK_NUM as usize);

    for _ in 0..TASK_NUM {
        let mut recv = recv.clone();
        let handle = set.spawn(async move { recv.changed().await.unwrap() });

        spawned.push(handle.id());
    }
    drop(recv);

    send.send_replace(());
    send.closed().await;

    let mut count = 0;
    let mut joined = Vec::with_capacity(TASK_NUM as usize);
    while let Some(res) = set.join_next_with_id().await {
        match res {
            Ok((id, ())) => {
                count += 1;
                joined.push(id);
            }
            Err(err) => panic!("failed: {err}"),
        }
    }

    assert_eq!(count, TASK_NUM);
    assert_eq!(joined, spawned);
}
