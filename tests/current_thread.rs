extern crate tokio;
extern crate tokio_executor;
extern crate futures;

use tokio::executor::current_thread::{self, run, CurrentThread};

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use futures::task;
use futures::future::lazy;
use futures::prelude::*;
use futures::sync::oneshot;

#[test]
fn spawning_from_init_future() {
    let cnt = Rc::new(Cell::new(0));

    run(|_| {
        let cnt = cnt.clone();

        current_thread::spawn(lazy(move || {
            cnt.set(1 + cnt.get());
            Ok(())
        }));
    });

    assert_eq!(1, cnt.get());
}

#[test]
fn run_seeded() {
    let cnt = Rc::new(Cell::new(0));
    let c = cnt.clone();

    let msg = current_thread::run_seeded(lazy(move || {
        c.set(1 + c.get());

        // Spawn!
        current_thread::spawn(lazy(move || {
            c.set(1 + c.get());
            Ok::<(), ()>(())
        }));

        Ok::<_, ()>("hello")
    })).unwrap();

    assert_eq!(2, cnt.get());
    assert_eq!(msg, "hello");
}

#[test]
fn block_waits() {
    let cnt = Rc::new(Cell::new(0));

    run(|_| {
        let cnt = cnt.clone();

        let (tx, rx) = oneshot::channel();

        thread::spawn(|| {
            thread::sleep(Duration::from_millis(1000));
            tx.send(()).unwrap();
        });

        current_thread::spawn(rx.then(move |_| {
            cnt.set(1 + cnt.get());
            Ok(())
        }));
    });

    assert_eq!(1, cnt.get());
}

#[test]
fn spawn_many() {
    const ITER: usize = 200;

    let cnt = Rc::new(Cell::new(0));

    run(|_| {
        for _ in 0..ITER {
            let cnt = cnt.clone();
            current_thread::spawn(lazy(move || {
                cnt.set(1 + cnt.get());
                Ok::<(), ()>(())
            }));
        }
    });

    assert_eq!(cnt.get(), ITER);
}

#[test]
fn does_not_set_global_executor_by_default() {
    run(|_| {
        // The execution context is setup, futures may be executed.
        current_thread::spawn(lazy(|| {
            println!("called from the current thread executor");
            Ok(())
        }));
    });
}

#[test]
fn spawn_from_block_on_future() {
    let cnt = Rc::new(Cell::new(0));

    let mut current_thread = CurrentThread::new();
    let mut enter = tokio_executor::enter().unwrap();

    current_thread.block_on(&mut enter, lazy(|| {
        let cnt = cnt.clone();

        current_thread::spawn(lazy(move || {
            cnt.set(1 + cnt.get());
            Ok(())
        }));

        Ok::<_, ()>(())
    })).unwrap();

    current_thread.run(&mut enter).unwrap();

    assert_eq!(1, cnt.get());
}

struct Never(Rc<()>);

impl Future for Never {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        Ok(Async::NotReady)
    }
}

#[test]
fn outstanding_tasks_are_dropped_when_executor_is_dropped() {
    let mut rc = Rc::new(());

    let mut current_thread = CurrentThread::new();
    current_thread.spawn(Never(rc.clone()));

    drop(current_thread);

    // Ensure the daemon is dropped
    assert!(Rc::get_mut(&mut rc).is_some());

    // Using the global spawn fn

    let mut rc = Rc::new(());

    let mut current_thread = CurrentThread::new();
    let mut enter = tokio_executor::enter().unwrap();

    current_thread.with_context(&mut enter, || {
        current_thread::spawn(Never(rc.clone()));
    });

    drop(current_thread);

    // Ensure the daemon is dropped
    assert!(Rc::get_mut(&mut rc).is_some());
}

#[test]
#[should_panic]
fn nesting_run() {
    run(|_| {
        run(|_| {
        });
    });
}

#[test]
#[should_panic]
fn run_in_future() {
    run(|_| {
        current_thread::spawn(lazy(|| {
            run(|_| {
            });
            Ok::<(), ()>(())
        }));
    });
}

#[test]
fn tasks_are_scheduled_fairly() {
    let state = Rc::new(RefCell::new([0, 0]));

    struct Spin {
        state: Rc<RefCell<[i32; 2]>>,
        idx: usize,
    }

    impl Future for Spin {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            let mut state = self.state.borrow_mut();

            if self.idx == 0 {
                let diff = state[0] - state[1];

                assert!(diff.abs() <= 1);

                if state[0] >= 50 {
                    return Ok(().into());
                }
            }

            state[self.idx] += 1;

            if state[self.idx] >= 100 {
                return Ok(().into());
            }

            task::current().notify();
            Ok(Async::NotReady)
        }
    }

    run(|_| {
        current_thread::spawn(Spin {
            state: state.clone(),
            idx: 0,
        });

        current_thread::spawn(Spin {
            state: state,
            idx: 1,
        });
    });
}
