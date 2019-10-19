use crate::task;
use crate::tests::loom_schedule::LoomSchedule;

use tokio_test::{assert_err, assert_ok};

use loom::future::block_on;
use loom::sync::atomic::AtomicBool;
use loom::sync::atomic::Ordering::{Acquire, Release};
use loom::thread;
use std::future::Future;

#[test]
fn create_drop_join_handle() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(async { "hello" });

        let schedule = LoomSchedule::new();
        let schedule = From::from(&schedule);

        let th = thread::spawn(move || {
            drop(join_handle);
        });

        assert_none!(task.run(schedule));

        th.join().unwrap();
    });
}

#[test]
fn poll_drop_handle_then_drop() {
    use futures_util::future::poll_fn;
    use std::pin::Pin;
    use std::task::Poll;

    loom::model(|| {
        let (task, mut join_handle) = task::joinable(async { "hello" });

        let schedule = LoomSchedule::new();
        let schedule = From::from(&schedule);

        let th = thread::spawn(move || {
            block_on(poll_fn(|cx| {
                let _ = Pin::new(&mut join_handle).poll(cx);
                Poll::Ready(())
            }));
        });

        assert_none!(task.run(schedule));

        th.join().unwrap();
    });
}

#[test]
fn join_output() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(async { "hello world" });

        let schedule = LoomSchedule::new();
        let schedule = From::from(&schedule);

        let th = thread::spawn(move || {
            let out = assert_ok!(block_on(join_handle));
            assert_eq!("hello world", out);
        });

        assert_none!(task.run(schedule));
        th.join().unwrap();
    });
}

#[test]
fn wake_by_ref() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(gated(2, true, false));

        let schedule = LoomSchedule::new();
        let schedule = &schedule;
        schedule.push_task(task);

        let th = join_one_task(join_handle);

        work(schedule);

        assert_ok!(th.join().unwrap());
    });
}

#[test]
fn wake_by_val() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(gated(2, true, true));

        let schedule = LoomSchedule::new();
        let schedule = &schedule;
        schedule.push_task(task);

        let th = join_one_task(join_handle);

        work(schedule);

        assert_ok!(th.join().unwrap());
    });
}

#[test]
fn release_remote() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(gated(1, false, true));

        let s1 = LoomSchedule::new();
        let s2 = LoomSchedule::new();

        // Join handle
        let th = join_one_task(join_handle);

        let task = match task.run(From::from(&s1)) {
            Some(task) => task,
            None => s1.recv().expect("released!"),
        };

        assert_none!(task.run(From::from(&s2)));
        assert_none!(s1.recv());

        assert_ok!(th.join().unwrap());
    });
}

#[test]
fn shutdown_task_before_poll() {
    loom::model(|| {
        let (task, join_handle) = task::joinable::<_, LoomSchedule>(async { "hello" });

        let th = join_one_task(join_handle);
        task.shutdown();

        assert_err!(th.join().unwrap());
    });
}

#[test]
fn shutdown_from_list_after_poll() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(gated(1, false, false));

        let s1 = LoomSchedule::new();

        let mut list = task::OwnedList::new();
        list.insert(&task);

        // Join handle
        let th = join_two_tasks(join_handle);

        match task.run(From::from(&s1)) {
            Some(task) => {
                // always drain the list before calling shutdown on tasks
                list.shutdown();

                // The task was scheduled, drain it explicitly.
                task.shutdown();
            }
            None => {
                list.shutdown();
            }
        };

        match s1.recv() {
            Some(task) => task.shutdown(),
            None => {}
        }

        assert_err!(th.join().unwrap());
    });
}

#[test]
fn shutdown_from_queue_after_poll() {
    loom::model(|| {
        let (task, join_handle) = task::joinable(gated(1, false, false));

        let s1 = LoomSchedule::new();

        // Join handle
        let th = join_two_tasks(join_handle);

        let task = match task.run(From::from(&s1)) {
            Some(task) => task,
            None => assert_some!(s1.recv()),
        };

        task.shutdown();

        assert_err!(th.join().unwrap());
    });
}

fn gated(n: usize, complete_first_poll: bool, by_val: bool) -> impl Future<Output = &'static str> {
    use futures_util::future::poll_fn;
    use std::sync::Arc;
    use std::task::Poll;

    let gate = Arc::new(AtomicBool::new(false));
    let mut fired = false;

    poll_fn(move |cx| {
        if !fired {
            for _ in 0..n {
                let gate = gate.clone();
                let waker = cx.waker().clone();
                thread::spawn(move || {
                    gate.store(true, Release);

                    if by_val {
                        waker.wake()
                    } else {
                        waker.wake_by_ref();
                    }
                });
            }

            fired = true;

            if !complete_first_poll {
                return Poll::Pending;
            }
        }

        if gate.load(Acquire) {
            Poll::Ready("hello world")
        } else {
            Poll::Pending
        }
    })
}

fn work(schedule: &LoomSchedule) {
    while let Some(task) = schedule.recv() {
        let mut task = Some(task);

        while let Some(t) = task.take() {
            task = t.run(From::from(schedule));
        }
    }
}

/// Spawn a thread to wait on the join handle. Uses a single task.
fn join_one_task<T: Future + 'static>(join_handle: T) -> loom::thread::JoinHandle<T::Output> {
    thread::spawn(move || block_on(join_handle))
}

/// Spawn a thread to wait on the join handle using two tasks. First, poll the
/// join handle on the first task. If the join handle is not ready, then use a
/// second task to wait on it.
fn join_two_tasks<T: Future + Unpin + 'static>(
    join_handle: T,
) -> loom::thread::JoinHandle<T::Output> {
    use futures_util::future::poll_fn;
    use std::task::Poll;

    // Join handle
    thread::spawn(move || {
        let mut join_handle = Some(join_handle);
        block_on(poll_fn(move |cx| {
            use std::pin::Pin;

            let res = Pin::new(join_handle.as_mut().unwrap()).poll(cx);

            if res.is_ready() {
                return res;
            }

            // Yes, we are nesting
            Poll::Ready(block_on(join_handle.take().unwrap()))
        }))
    })
}
