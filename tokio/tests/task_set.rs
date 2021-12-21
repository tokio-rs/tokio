#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::sync::oneshot;
use tokio::task::TaskSet;
use tokio::time::Duration;

#[tokio::test(start_paused = true)]
async fn test_with_sleep() {
    let mut set = TaskSet::new();

    assert!(matches!(set.join_one().await, Ok(None)));

    for i in 0..10 {
        set.spawn(async move {
            tokio::time::sleep(Duration::from_secs(i as u64)).await;
            i
        });
        assert_eq!(set.len(), 1 + i);
    }

    let mut seen = [false; 10];
    while let Some(res) = set.join_one().await.unwrap() {
        seen[res] = true;
    }

    for i in 0..10 {
        assert!(seen[i]);
    }
    assert!(matches!(set.join_one().await, Ok(None)));

    // Do it again.
    for i in 0..10 {
        set.spawn(async move {
            tokio::time::sleep(Duration::from_secs(i as u64)).await;
            i
        });
    }

    let mut seen = [false; 10];
    while let Some(res) = set.join_one().await.unwrap() {
        seen[res] = true;
    }

    for i in 0..10 {
        assert!(seen[i]);
    }
    assert!(matches!(set.join_one().await, Ok(None)));
}

#[tokio::test]
async fn test_abort_on_drop() {
    let mut set = TaskSet::new();

    let mut recvs = Vec::new();

    for _ in 0..16 {
        let (send, recv) = oneshot::channel::<()>();
        recvs.push(recv);

        set.spawn(async {
            // This task will never complete on its own.
            futures::future::pending::<()>().await;
            drop(send);
        });
    }

    drop(set);

    for recv in recvs {
        // The task is aborted soon and we will receive an error.
        assert!(recv.await.is_err());
    }
}

#[tokio::test]
async fn alternating() {
    let mut set = TaskSet::new();

    assert_eq!(set.len(), 0);
    set.spawn(async { () });
    assert_eq!(set.len(), 1);
    set.spawn(async { () });
    assert_eq!(set.len(), 2);

    for _ in 0..16 {
        let () = set.join_one().await.unwrap().unwrap();
        assert_eq!(set.len(), 1);
        set.spawn(async { () });
        assert_eq!(set.len(), 2);
    }
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}

#[test]
fn runtime_gone() {
    let mut set = TaskSet::new();
    {
        let rt = rt();
        set.spawn_on(async { 1 }, rt.handle());
        drop(rt);
    }

    assert!(rt().block_on(set.join_one()).unwrap_err().is_cancelled());
}
