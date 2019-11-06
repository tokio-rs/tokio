use crate::runtime::task::{self, Header};
use crate::runtime::tests::backoff::*;
use crate::runtime::tests::mock_schedule::{mock, Mock};
use crate::runtime::tests::track_drop::track_drop;
use crate::sync::oneshot;

use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready_err, assert_ready_ok};

use futures_util::future::poll_fn;
use std::sync::mpsc;

#[test]
fn header_lte_cache_line() {
    use std::mem::size_of;

    assert!(size_of::<Header>() <= 8 * size_of::<*const ()>());
}

#[test]
fn create_complete_drop() {
    let (tx, rx) = mpsc::channel();

    let (task, did_drop) = track_drop(async move {
        tx.send(1).unwrap();
    });

    let task = task::background(task);

    let mock = mock().bind(&task).release_local();
    let mock = &mut || Some(From::from(&mock));

    // Nothing is returned
    assert!(task.run(mock).is_none());

    // The message was sent
    assert!(rx.try_recv().is_ok());

    // The future & output were dropped.
    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn create_yield_complete_drop() {
    let (tx, rx) = mpsc::channel();

    let (task, did_drop) = track_drop(async move {
        backoff(1).await;
        tx.send(1).unwrap();
    });

    let task = task::background(task);

    let mock = mock().bind(&task).release_local();
    let mock = || Some(From::from(&mock));

    // Task is returned
    let task = assert_some!(task.run(mock));

    // The future was **not** dropped.
    assert!(!did_drop.did_drop_future());

    assert_none!(task.run(mock));

    // The message was sent
    assert!(rx.try_recv().is_ok());

    // The future was dropped.
    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn create_clone_yield_complete_drop() {
    let (tx, rx) = mpsc::channel();

    let (task, did_drop) = track_drop(async move {
        backoff_clone(1).await;
        tx.send(1).unwrap();
    });

    let task = task::background(task);

    let mock = mock().bind(&task).release_local();
    let mock = || Some(From::from(&mock));

    // Task is returned
    let task = assert_some!(task.run(mock));

    // The future was **not** dropped.
    assert!(!did_drop.did_drop_future());

    assert_none!(task.run(mock));

    // The message was sent
    assert!(rx.try_recv().is_ok());

    // The future was dropped.
    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn create_wake_drop() {
    let (tx, rx) = oneshot::channel();

    let (task, did_drop) = track_drop(async move { rx.await });

    let task = task::background(task);

    let mock = mock().bind(&task).schedule().release_local();

    assert_none!(task.run(&mut || Some(From::from(&mock))));
    assert_none!(mock.next_pending_run());

    // The future was **not** dropped.
    assert!(!did_drop.did_drop_future());

    tx.send("hello").unwrap();

    let task = assert_some!(mock.next_pending_run());

    assert_none!(task.run(&mut || Some(From::from(&mock))));

    // The future was dropped.
    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn notify_complete() {
    use std::task::Poll::Ready;

    let (task, did_drop) = track_drop(async move {
        poll_fn(|cx| {
            cx.waker().wake_by_ref();
            Ready(())
        })
        .await;
    });

    let task = task::background(task);

    let mock = mock().bind(&task).release_local();
    let mock = &mut || Some(From::from(&mock));

    assert_none!(task.run(mock));
    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn complete_on_second_schedule_obj() {
    let (tx, rx) = mpsc::channel();

    let (task, did_drop) = track_drop(async move {
        backoff(1).await;
        tx.send(1).unwrap();
    });

    let task = task::background(task);

    let mock1 = mock();
    let mock2 = mock().bind(&task).release();

    // Task is returned
    let task = assert_some!(task.run(&mut || Some(From::from(&mock2))));

    assert_none!(task.run(&mut || Some(From::from(&mock1))));

    // The message was sent
    assert!(rx.try_recv().is_ok());

    // The future was dropped.
    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());

    let _ = assert_some!(mock2.next_pending_drop());
}

#[test]
fn join_task_immediate_drop_handle() {
    let (task, did_drop) = track_drop(async move { "hello".to_string() });

    let (task, _) = task::joinable(task);

    let mock = mock().bind(&task).release_local();

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());

    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn join_task_immediate_complete_1() {
    let (task, did_drop) = track_drop(async move { "hello".to_string() });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    let mock = mock().bind(&task).release_local();

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());

    assert!(did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());
    assert!(!handle.is_woken());

    let out = assert_ready_ok!(handle.poll());
    assert_eq!(out.get_ref(), "hello");

    drop(out);

    assert!(did_drop.did_drop_output());
}

#[test]
fn join_task_immediate_complete_2() {
    let (task, did_drop) = track_drop(async move { "hello".to_string() });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    let mock = mock().bind(&task).release_local();

    assert_pending!(handle.poll());

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());

    assert!(did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());
    assert!(handle.is_woken());

    let out = assert_ready_ok!(handle.poll());
    assert_eq!(out.get_ref(), "hello");

    drop(out);

    assert!(did_drop.did_drop_output());
}

#[test]
fn join_task_complete_later() {
    let (task, did_drop) = track_drop(async move {
        backoff(1).await;
        "hello".to_string()
    });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(async { handle.await });

    let mock = mock().bind(&task).release_local();

    let task = assert_some!(task.run(&mut || Some(From::from(&mock))));

    assert!(!did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());

    assert_pending!(handle.poll());

    assert_none!(task.run(&mut || Some(From::from(&mock))));
    assert!(handle.is_woken());

    let out = assert_ready_ok!(handle.poll());
    assert_eq!(out.get_ref(), "hello");

    drop(out);

    assert!(did_drop.did_drop_output());

    assert_eq!(1, handle.waker_ref_count());
}

#[test]
fn drop_join_after_poll() {
    let (task, did_drop) = track_drop(async move {
        backoff(1).await;
        "hello".to_string()
    });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(async { handle.await });

    let mock = mock().bind(&task).release_local();

    assert_pending!(handle.poll());
    drop(handle);

    let task = assert_some!(task.run(&mut || Some(From::from(&mock))));

    assert!(!did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());

    assert_none!(task.run(&mut || Some(From::from(&mock))));

    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
fn join_handle_change_task_complete() {
    use std::future::Future;
    use std::pin::Pin;

    let (task, did_drop) = track_drop(async move {
        backoff(1).await;
        "hello".to_string()
    });

    let (task, mut handle) = task::joinable(task);
    let mut t1 = spawn(poll_fn(|cx| Pin::new(&mut handle).poll(cx)));

    let mock = mock().bind(&task).release_local();

    assert_pending!(t1.poll());
    drop(t1);

    let task = assert_some!(task.run(&mut || Some(From::from(&mock))));

    let mut t2 = spawn(poll_fn(|cx| Pin::new(&mut handle).poll(cx)));
    assert_pending!(t2.poll());

    assert!(!did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());

    assert_none!(task.run(&mut || Some(From::from(&mock))));

    assert!(t2.is_woken());

    let out = assert_ready_ok!(t2.poll());
    assert_eq!(out.get_ref(), "hello");

    drop(out);

    assert!(did_drop.did_drop_output());

    assert_eq!(1, t2.waker_ref_count());
}

#[test]
fn drop_handle_after_complete() {
    let (task, did_drop) = track_drop(async move { "hello".to_string() });

    let (task, handle) = task::joinable(task);

    let mock = mock().bind(&task).release_local();

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());

    assert!(did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());

    drop(handle);

    assert!(did_drop.did_drop_output());
}

#[test]
fn non_initial_task_state_drop_join_handle_without_polling() {
    let (tx, rx) = oneshot::channel::<()>();

    let (task, did_drop) = track_drop(async move {
        rx.await.unwrap();
        "hello".to_string()
    });

    let (task, handle) = task::joinable(task);

    let mock = mock().bind(&task).schedule().release_local();

    assert_none!(task.run(&mut || Some(From::from(&mock))));

    drop(handle);

    assert!(!did_drop.did_drop_future());
    assert!(!did_drop.did_drop_output());

    tx.send(()).unwrap();
    let task = assert_some!(mock.next_pending_run());

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());

    assert!(did_drop.did_drop_future());
    assert!(did_drop.did_drop_output());
}

#[test]
#[cfg(not(miri))]
fn task_panic_background() {
    let (task, did_drop) = track_drop(async move {
        if true {
            panic!()
        }
        "hello"
    });

    let task = task::background(task);

    let mock = mock().bind(&task).release_local();

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());

    assert!(did_drop.did_drop_future());
}

#[test]
#[cfg(not(miri))]
fn task_panic_join() {
    let (task, did_drop) = track_drop(async move {
        if true {
            panic!()
        }
        "hello"
    });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    let mock = mock().bind(&task).release_local();

    assert_pending!(handle.poll());

    assert!(task.run(&mut || Some(From::from(&mock))).is_none());
    assert!(did_drop.did_drop_future());
    assert!(handle.is_woken());

    assert_ready_err!(handle.poll());
}

#[test]
fn complete_second_schedule_obj_before_join() {
    let (tx, rx) = oneshot::channel();

    let (task, did_drop) = track_drop(async move { rx.await.unwrap() });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    let mock1 = mock();
    let mock2 = mock().bind(&task).schedule().release();

    assert_pending!(handle.poll());

    assert_none!(task.run(&mut || Some(From::from(&mock2))));

    tx.send("hello").unwrap();

    let task = assert_some!(mock2.next_pending_run());
    assert_none!(task.run(&mut || Some(From::from(&mock1))));
    assert!(did_drop.did_drop_future());

    // The join handle was notified
    assert!(handle.is_woken());

    // Drop the task
    let _ = assert_some!(mock2.next_pending_drop());

    // Get the output
    let out = assert_ready_ok!(handle.poll());
    assert_eq!(*out.get_ref(), "hello");
}

#[test]
fn complete_second_schedule_obj_after_join() {
    let (tx, rx) = oneshot::channel();

    let (task, did_drop) = track_drop(async move { rx.await.unwrap() });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    let mock1 = mock();
    let mock2 = mock().bind(&task).schedule().release();

    assert_pending!(handle.poll());

    assert_none!(task.run(&mut || Some(From::from(&mock2))));

    tx.send("hello").unwrap();

    let task = assert_some!(mock2.next_pending_run());
    assert_none!(task.run(&mut || Some(From::from(&mock1))));
    assert!(did_drop.did_drop_future());

    // The join handle was notified
    assert!(handle.is_woken());

    // Get the output
    let out = assert_ready_ok!(handle.poll());
    assert_eq!(*out.get_ref(), "hello");

    // Drop the task
    let _ = assert_some!(mock2.next_pending_drop());

    assert_eq!(1, handle.waker_ref_count());
}

#[test]
fn shutdown_from_list_before_notified() {
    let (tx, rx) = oneshot::channel::<()>();
    let mut list = task::OwnedList::new();

    let (task, did_drop) = track_drop(async move { rx.await });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    list.insert(&task);

    let mock = mock().bind(&task).release();

    assert_pending!(handle.poll());
    assert_none!(task.run(&mut || Some(From::from(&mock))));

    list.shutdown();
    assert!(did_drop.did_drop_future());

    assert!(handle.is_woken());

    let task = assert_some!(mock.next_pending_drop());
    drop(task);

    assert_ready_err!(handle.poll());

    drop(tx);
}

#[test]
fn shutdown_from_list_after_notified() {
    let (tx, rx) = oneshot::channel::<()>();
    let mut list = task::OwnedList::new();

    let (task, did_drop) = track_drop(async move { rx.await });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    list.insert(&task);

    let mock = mock().bind(&task).schedule().release();

    assert_pending!(handle.poll());
    assert_none!(task.run(&mut || Some(From::from(&mock))));

    tx.send(()).unwrap();

    let task = assert_some!(mock.next_pending_run());

    list.shutdown();

    assert_none!(mock.next_pending_drop());

    assert_none!(task.run(&mut || Some(From::from(&mock))));
    assert!(did_drop.did_drop_future());
    assert!(handle.is_woken());

    let task = assert_some!(mock.next_pending_drop());
    drop(task);

    assert_ready_err!(handle.poll());
}

#[test]
fn shutdown_from_list_after_complete() {
    let mut list = task::OwnedList::new();

    let (task, did_drop) = track_drop(async move {
        backoff(1).await;
        "hello"
    });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    list.insert(&task);

    let m1 = mock().bind(&task).release();
    let m2 = mock();

    assert_pending!(handle.poll());
    let task = assert_some!(task.run(&mut || Some(From::from(&m1))));
    assert_none!(task.run(&mut || Some(From::from(&m2))));
    assert!(did_drop.did_drop_future());
    assert!(handle.is_woken());

    list.shutdown();

    let task = assert_some!(m1.next_pending_drop());
    drop(task);

    let out = assert_ready_ok!(handle.poll());
    assert_eq!(*out.get_ref(), "hello");
}

#[test]
fn shutdown_from_task_before_notified() {
    let (tx, rx) = oneshot::channel::<()>();

    let (task, did_drop) = track_drop(async move { rx.await });

    let (task, handle) = task::joinable::<_, Mock>(task);
    let mut handle = spawn(handle);

    assert_pending!(handle.poll());

    task.shutdown();
    assert!(did_drop.did_drop_future());
    assert!(handle.is_woken());

    assert_ready_err!(handle.poll());

    drop(tx);
}

#[test]
fn shutdown_from_task_after_notified() {
    let (tx, rx) = oneshot::channel::<()>();

    let (task, did_drop) = track_drop(async move { rx.await });

    let (task, handle) = task::joinable(task);
    let mut handle = spawn(handle);

    let mock = mock().bind(&task).schedule().release();

    assert_pending!(handle.poll());
    assert_none!(task.run(&mut || Some(From::from(&mock))));

    tx.send(()).unwrap();

    let task = assert_some!(mock.next_pending_run());

    task.shutdown();
    assert!(did_drop.did_drop_future());
    assert!(handle.is_woken());

    let task = assert_some!(mock.next_pending_drop());
    drop(task);

    assert_ready_err!(handle.poll());
}
