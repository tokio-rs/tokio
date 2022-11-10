#![warn(rust_2018_idioms)]
#![allow(clippy::declare_interior_mutable_const)]
#![cfg(all(feature = "full", not(tokio_wasi)))]

#[cfg(tokio_unstable)]
use std::error::Error;
#[cfg(tokio_unstable)]
use std::future::Future;
#[cfg(tokio_unstable)]
use std::pin::Pin;
#[cfg(tokio_unstable)]
use std::task::{Context, Poll};
#[cfg(tokio_unstable)]
use tokio::runtime::{Builder, Runtime};
#[cfg(tokio_unstable)]
use tokio::sync::oneshot;
#[cfg(tokio_unstable)]
use tokio::task::{self, LocalSet};

#[cfg(tokio_unstable)]
mod support {
    pub mod panic;
}
#[cfg(tokio_unstable)]
use support::panic::test_panic;

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_spawn() {
    tokio::spawn(async { println!("task id: {}", task::id()) })
        .await
        .unwrap();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_spawn_blocking() {
    task::spawn_blocking(|| println!("task id: {}", task::id()))
        .await
        .unwrap();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_collision_current_thread() {
    let handle1 = tokio::spawn(async { task::id() });
    let handle2 = tokio::spawn(async { task::id() });

    let (id1, id2) = tokio::join!(handle1, handle2);
    assert_ne!(id1.unwrap(), id2.unwrap());
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "multi_thread")]
async fn task_id_collision_multi_thread() {
    let handle1 = tokio::spawn(async { task::id() });
    let handle2 = tokio::spawn(async { task::id() });

    let (id1, id2) = tokio::join!(handle1, handle2);
    assert_ne!(id1.unwrap(), id2.unwrap());
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_ids_match_current_thread() {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(async {
        let id = rx.await.unwrap();
        assert_eq!(id, task::id());
    });
    tx.send(handle.id()).unwrap();
    handle.await.unwrap();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "multi_thread")]
async fn task_ids_match_multi_thread() {
    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(async {
        let id = rx.await.unwrap();
        assert_eq!(id, task::id());
    });
    tx.send(handle.id()).unwrap();
    handle.await.unwrap();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "multi_thread")]
async fn task_id_future_destructor_completion() {
    struct MyFuture;

    impl Future for MyFuture {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            Poll::Ready(())
        }
    }

    impl Drop for MyFuture {
        fn drop(&mut self) {
            println!("task id: {}", task::id());
        }
    }

    tokio::spawn(MyFuture).await.unwrap();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "multi_thread")]
async fn task_id_future_destructor_abort() {
    struct MyFuture;

    impl Future for MyFuture {
        type Output = ();

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
            Poll::Pending
        }
    }
    impl Drop for MyFuture {
        fn drop(&mut self) {
            println!("task id: {}", task::id());
        }
    }

    tokio::spawn(MyFuture).abort();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_output_destructor_handle_dropped_before_completion() {
    struct MyOutput;

    impl Drop for MyOutput {
        fn drop(&mut self) {
            println!("task id: {}", task::id());
        }
    }

    struct MyFuture {
        tx: Option<oneshot::Sender<()>>,
    }

    impl Future for MyFuture {
        type Output = MyOutput;

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            let _ = self.tx.take().unwrap().send(());
            Poll::Ready(MyOutput)
        }
    }

    impl Drop for MyFuture {
        fn drop(&mut self) {
            println!("task id: {}", task::id());
        }
    }

    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(MyFuture { tx: Some(tx) });
    drop(handle);
    rx.await.unwrap();
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_output_destructor_handle_dropped_after_completion() {
    struct MyOutput;

    impl Drop for MyOutput {
        fn drop(&mut self) {
            println!("task id: {}", task::id());
        }
    }

    struct MyFuture {
        tx: Option<oneshot::Sender<()>>,
    }

    impl Future for MyFuture {
        type Output = MyOutput;

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            let _ = self.tx.take().unwrap().send(());
            Poll::Ready(MyOutput)
        }
    }

    impl Drop for MyFuture {
        fn drop(&mut self) {
            println!("task id: {}", task::id());
        }
    }

    let (tx, rx) = oneshot::channel();
    let handle = tokio::spawn(MyFuture { tx: Some(tx) });
    rx.await.unwrap();
    drop(handle);
}

#[cfg(tokio_unstable)]
#[test]
fn task_try_id_outside_task() {
    assert_eq!(None, task::try_id());
}

#[cfg(tokio_unstable)]
#[test]
fn task_try_id_inside_block_on() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        assert_eq!(None, task::try_id());
    });
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_spawn_local() {
    LocalSet::new()
        .run_until(async {
            task::spawn_local(async { println!("task id: {}", task::id()) })
                .await
                .unwrap();
        })
        .await
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "current_thread")]
async fn task_id_nested_spawn_local() {
    LocalSet::new()
        .run_until(async {
            task::spawn_local(async {
                let outer_id = task::id();
                LocalSet::new()
                    .run_until(async {
                        task::spawn_local(async move {
                            assert_ne!(outer_id, task::id());
                        })
                        .await
                        .unwrap();
                    })
                    .await;
                assert_eq!(outer_id, task::id());
            })
            .await
            .unwrap();
        })
        .await;
}

#[cfg(tokio_unstable)]
#[test]
fn task_id_outside_task_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let _ = task::id();
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[cfg(tokio_unstable)]
#[test]
fn task_id_inside_block_on_panic_caller() -> Result<(), Box<dyn Error>> {
    let panic_location_file = test_panic(|| {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            task::id();
        });
    });

    // The panic location should be in this file
    assert_eq!(&panic_location_file.unwrap(), file!());

    Ok(())
}

#[cfg(tokio_unstable)]
#[tokio::test(flavor = "multi_thread")]
async fn task_id_block_in_place_block_on_spawn() {
    task::spawn(async {
        let id1 = task::id();

        task::block_in_place(|| {
            let rt = Builder::new_current_thread().build().unwrap();
            rt.block_on(rt.spawn(async {})).unwrap();
        });

        let id2 = task::id();
        assert_eq!(id1, id2);
    })
    .await
    .unwrap();
}
