#![warn(rust_2018_idioms)]
#![cfg(all(tokio_unstable, feature = "async-scope", not(target_family = "wasm")))]

use tokio::task::spawn_scoped;

#[tokio::test(flavor = "multi_thread")]
async fn test_basic_scope() {
    let data = 8;

    let scope_handle = spawn_scoped(data, |_scope, data_inner| Box::pin(async move {
       *data_inner + 1
    }));

    let (sum, _) = scope_handle.await.unwrap();

    assert_eq!(sum, 9);
}


#[tokio::test(flavor = "multi_thread")]
async fn test_scoped_tasks() {
    let data = vec!["1", "2", "3", "4"];

    let scope_handle = spawn_scoped(data, |scope, data_inner| Box::pin(async move {
        let mut tasks = vec![];
        for i in data_inner.iter() {
            let task = scope.spawn(async {
                i.parse::<i32>().unwrap()
            }).await;
            tasks.push(task);
        }

        let mut sum = 0;
        
        for task in tasks {
            sum += task.await.unwrap();
        }

        sum
    }));

    let (sum, _) = scope_handle.await.unwrap();

    assert_eq!(sum, 10);
}


#[tokio::test(flavor = "multi_thread")]
async fn test_nested_scoped_tasks() {
    let data = "scopes are ".to_string();

    let scope_handle = spawn_scoped(data, |scope, data_inner| Box::pin(async move {
        scope.spawn(async move {
            scope.spawn(async move {
                scope.spawn(async move {
                    5
                }).await.await.unwrap()
            }).await.await.unwrap()
        }).await.await.unwrap()
    }));

    let (num, _) = scope_handle.await.unwrap();

    assert_eq!(num, 5);
}


// test UAF prevention, make a box, spawn scoped task that sleeps and then reads the box
#[tokio::test(flavor = "multi_thread")]
async fn test_scoped_tasks_uaf() {
    let data = Box::new(8);

    let scope_handle = spawn_scoped(data, |scope, data_inner| Box::pin(async move {
        scope.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            **data_inner
        }).await;
    }));

    scope_handle.await.unwrap();
}

// same as above test, but spawns a runtime manually, then shuts down and ensures proper cleanup of scoped resources
#[test]
fn test_scoped_tasks_uaf_manual_runtime() {
    use tokio::runtime::Builder;

    struct DropGuarded {
        sender: Option<tokio::sync::oneshot::Sender<()>>,
    }

    impl Drop for DropGuarded {
        fn drop(&mut self) {
            if let Some(sender) = self.sender.take() {
                sender.send(()).unwrap();
            }
        }
    }

    let (tx, mut rx) = tokio::sync::oneshot::channel();

    let rt = Builder::new_current_thread().enable_time().build().unwrap();

    rt.block_on(async {
        let data = DropGuarded { sender: Some(tx) };
        let _ = spawn_scoped(data, |scope, data_inner| Box::pin(async move {
            let task = scope.spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                // this wont run because runtime will be dropped before sleep finishes
                unreachable!();
            }).await;

            task.await.unwrap()
        }));
    });

    drop(rt);

    rx.try_recv().expect("Expected DropGuarded to be dropped and send on the channel");
}