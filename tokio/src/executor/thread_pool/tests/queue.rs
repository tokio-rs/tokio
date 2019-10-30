use crate::executor::task::{self, Task};
use crate::executor::tests::mock_schedule::{Noop, NOOP_SCHEDULE};
use crate::executor::thread_pool::{queue, LOCAL_QUEUE_CAPACITY};

macro_rules! assert_pop {
    ($q:expr, $expect:expr) => {
        assert_eq!(
            match $q.pop_local_first() {
                Some(v) => num(v),
                None => panic!("queue empty"),
            },
            $expect
        )
    };
}

macro_rules! assert_pop_global {
    ($q:expr, $expect:expr) => {
        assert_eq!(
            match $q.pop_global_first() {
                Some(v) => num(v),
                None => panic!("queue empty"),
            },
            $expect
        )
    };
}

macro_rules! assert_steal {
    ($q:expr, $n:expr, $expect:expr) => {
        assert_eq!(
            match $q.steal($n) {
                Some(v) => num(v),
                None => panic!("queue empty"),
            },
            $expect
        )
    };
}

macro_rules! assert_empty {
    ($q:expr) => {{
        let q: &mut queue::Worker<Noop> = &mut $q;
        match q.pop_local_first() {
            Some(v) => panic!("expected emtpy queue; got {}", num(v)),
            None => {}
        }
    }};
}

#[test]
fn single_worker_push_pop() {
    let mut q = queue::build(1).remove(0);

    // Queue is empty
    assert_empty!(q);

    // Push a value
    q.push(val(0));

    // Pop the value
    assert_pop!(q, 0);

    // Push two values
    q.push(val(1));
    q.push(val(2));
    q.push(val(3));

    // Pop the value
    assert_pop!(q, 3);
    assert_pop!(q, 1);
    assert_pop!(q, 2);
    assert_empty!(q);
}

#[test]
fn multi_worker_push_pop() {
    let (mut q1, mut q2) = queues_2();

    // Queue is empty
    assert_empty!(q1);
    assert_empty!(q2);

    // Push a value
    q1.push(val(0));

    // Not available on other queue
    assert_empty!(q2);
    assert_pop!(q1, 0);

    q2.push(val(1));
    assert_pop!(q2, 1);
    assert_empty!(q1);
}

#[test]
fn multi_worker_inject_pop() {
    let (mut q1, mut q2) = queues_2();
    let i = q1.injector();

    // Push a value
    i.push(val(0), is_ok);
    assert_pop!(q1, 0);
    assert_empty!(q2);

    // Push another value
    i.push(val(1), is_ok);
    assert_pop!(q2, 1);
    assert_empty!(q1);

    i.push(val(2), is_ok);
    i.push(val(3), is_ok);
    i.push(val(4), is_ok);
    assert_pop!(q2, 2);
    assert_pop!(q1, 3);
    assert_pop!(q1, 4);
}

#[test]
fn overflow_local_queue() {
    let (mut q1, mut q2) = queues_2();

    for i in 0..LOCAL_QUEUE_CAPACITY {
        q1.push(val(i as u32));
    }

    assert_empty!(q2);

    // Fill `next` slot
    q1.push(val(999));

    // overflow
    q1.push(val(1000));

    assert_pop!(q2, 0);
    assert_pop!(q1, 1000);

    // Half the values were moved to the global queue
    for i in 128..LOCAL_QUEUE_CAPACITY {
        assert_pop!(q1, i as u32);
    }

    for i in 1..128 {
        assert_pop!(q2, i);
    }

    assert_pop!(q2, 999);
    assert_empty!(q2);

    assert_empty!(q1);
}

#[test]
fn polling_global_first() {
    let (q, _) = queues_2();
    let i = q.injector();

    i.push(val(1000), is_ok);
    i.push(val(1001), is_ok);

    for n in 0..5 {
        q.push(val(n));
    }

    assert_pop_global!(q, 1000);
    assert_pop!(q, 4);
    assert_pop_global!(q, 1001);
    assert_pop_global!(q, 0);
    assert_pop!(q, 1);
    assert_pop_global!(q, 2);
    assert_pop_global!(q, 3);

    assert!(q.pop_global_first().is_none());
}

#[test]
fn steal() {
    let mut qs = queue::build(3);
    let (mut q1, mut q2, mut q3) = (qs.remove(0), qs.remove(0), qs.remove(0));

    assert!(q1.steal(0).is_none());
    assert!(q2.steal(0).is_none());
    assert!(q3.steal(0).is_none());

    // Steal one value, but not the first one
    q1.push(val(0));
    q1.push(val(999));
    assert_steal!(q2, 0, 0);
    assert!(q2.steal(0).is_none());
    assert_pop!(q1, 999);

    // Steals half the queue
    for i in 0..4 {
        q1.push(val(i));
    }

    q1.push(val(999));

    assert_steal!(q2, 0, 1);
    assert_pop!(q2, 0);
    assert_empty!(q2);
    assert_pop!(q1, 999);
    assert_pop!(q1, 2);
    assert_pop!(q1, 3);
    assert_empty!(q1);

    // Searches multiple queues
    q3.push(val(0));
    q3.push(val(999));
    assert_steal!(q2, 0, 0);
    assert_pop!(q3, 999);
    assert_empty!(q3);

    // Steals from one queue at a time
    q1.push(val(0));
    q1.push(val(998));
    q2.push(val(1));
    q2.push(val(999));

    assert_steal!(q3, 0, 0);
    assert_pop!(q2, 999);
    assert_pop!(q2, 1);
    assert_empty!(q2);

    assert_pop!(q1, 998);
    assert_empty!(q1);
}

fn queues_2() -> (queue::Worker<Noop>, queue::Worker<Noop>) {
    let mut qs = queue::build(2);
    (qs.remove(0), qs.remove(0))
}

// pretty big hack to track tasks
use std::cell::RefCell;
use std::collections::HashMap;
thread_local! {
    static TASKS: RefCell<HashMap<u32, task::JoinHandle<u32, Noop>>> = RefCell::new(HashMap::new())
}

fn val(num: u32) -> Task<Noop> {
    let (task, join) = task::joinable(async move { num });
    let prev = TASKS.with(|t| t.borrow_mut().insert(num, join));
    assert!(prev.is_none());
    task
}

fn num(task: Task<Noop>) -> u32 {
    use futures_util::task::noop_waker_ref;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::Context;
    use std::task::Poll::*;

    assert!(task.run(&mut || Some(From::from(&NOOP_SCHEDULE))).is_none());

    // Find the task that completed
    TASKS.with(|c| {
        let mut map = c.borrow_mut();
        let mut num = None;

        for (_, join) in map.iter_mut() {
            let mut cx = Context::from_waker(noop_waker_ref());
            match Pin::new(join).poll(&mut cx) {
                Ready(n) => {
                    num = Some(n.unwrap());
                    break;
                }
                _ => {}
            }
        }

        let num = num.expect("no task completed");
        map.remove(&num);
        num
    })
}

fn is_ok<T, E>(r: Result<T, E>) {
    assert!(r.is_ok())
}
