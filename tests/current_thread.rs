extern crate tokio;
extern crate tokio_executor;
extern crate futures;

use tokio::executor::current_thread::{self, block_on_all, CurrentThread};

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use futures::task;
use futures::future::{self, lazy};
use futures::prelude::*;
use futures::sync::oneshot;

#[test]
fn spawn_from_block_on_all() {
    let cnt = Rc::new(Cell::new(0));
    let c = cnt.clone();

    let msg = current_thread::block_on_all(lazy(move || {
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
    let cnt2 = cnt.clone();

    let (tx, rx) = oneshot::channel();

    thread::spawn(|| {
        thread::sleep(Duration::from_millis(1000));
        tx.send(()).unwrap();
    });

    block_on_all(rx.then(move |_| {
        cnt.set(1 + cnt.get());
        Ok::<_, ()>(())
    })).unwrap();

    assert_eq!(1, cnt2.get());
}

#[test]
fn spawn_many() {
    const ITER: usize = 200;

    let cnt = Rc::new(Cell::new(0));
    let mut current_thread = CurrentThread::new();

    for _ in 0..ITER {
        let cnt = cnt.clone();
        current_thread.spawn(lazy(move || {
            cnt.set(1 + cnt.get());
            Ok::<(), ()>(())
        }));
    }

    current_thread.run().unwrap();

    assert_eq!(cnt.get(), ITER);
}

#[test]
fn does_not_set_global_executor_by_default() {
    use tokio_executor::Executor;

    block_on_all(lazy(|| {
        tokio_executor::DefaultExecutor::current()
            .spawn(Box::new(lazy(|| ok())))
            .unwrap_err();

        ok()
    })).unwrap();
}

#[test]
fn spawn_from_block_on_future() {
    let cnt = Rc::new(Cell::new(0));

    let mut current_thread = CurrentThread::new();

    current_thread.block_on(lazy(|| {
        let cnt = cnt.clone();

        current_thread::spawn(lazy(move || {
            cnt.set(1 + cnt.get());
            Ok(())
        }));

        Ok::<_, ()>(())
    })).unwrap();

    current_thread.run().unwrap();

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

    current_thread.block_on(lazy(|| {
        current_thread::spawn(Never(rc.clone()));
        Ok::<_, ()>(())
    })).unwrap();

    drop(current_thread);

    // Ensure the daemon is dropped
    assert!(Rc::get_mut(&mut rc).is_some());
}

#[test]
#[should_panic]
fn nesting_run() {
    block_on_all(lazy(|| {
        block_on_all(lazy(|| {
            ok()
        })).unwrap();

        ok()
    })).unwrap();
}

#[test]
#[should_panic]
fn run_in_future() {
    block_on_all(lazy(|| {
        current_thread::spawn(lazy(|| {
            block_on_all(lazy(|| {
                ok()
            })).unwrap();
            ok()
        }));
        ok()
    })).unwrap();
}

#[test]
fn tick_on_infini_future() {
    let num = Rc::new(Cell::new(0));

    struct Infini {
        num: Rc<Cell<usize>>,
    }

    impl Future for Infini {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            self.num.set(1 + self.num.get());
            task::current().notify();
            Ok(Async::NotReady)
        }
    }

    CurrentThread::new()
        .spawn(Infini {
            num: num.clone(),
        })
        .turn(None)
        .unwrap();

    assert_eq!(1, num.get());
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

    block_on_all(lazy(|| {
        current_thread::spawn(Spin {
            state: state.clone(),
            idx: 0,
        });

        current_thread::spawn(Spin {
            state: state,
            idx: 1,
        });

        ok()
    })).unwrap();
}

#[test]
fn spawn_and_turn() {
    let cnt = Rc::new(Cell::new(0));
    let c = cnt.clone();

    let mut current_thread = CurrentThread::new();

    // Spawn a basic task to get the executor to turn
    current_thread.spawn(lazy(move || {
        Ok(())
    }));

    // Turn once...
    current_thread.turn(None).unwrap();

    current_thread.spawn(lazy(move || {
        c.set(1 + c.get());

        // Spawn!
        current_thread::spawn(lazy(move || {
            c.set(1 + c.get());
            Ok::<(), ()>(())
        }));

        Ok(())
    }));

    // This does not run the newly spawned thread
    current_thread.turn(None).unwrap();
    assert_eq!(1, cnt.get());

    // This runs the newly spawned thread
    current_thread.turn(None).unwrap();
    assert_eq!(2, cnt.get());
}

#[test]
fn spawn_in_drop() {
    let mut current_thread = CurrentThread::new();

    let (tx, rx) = oneshot::channel();

    struct OnDrop<F: FnOnce()>(Option<F>);

    impl<F: FnOnce()> Drop for OnDrop<F> {
        fn drop(&mut self) {
            (self.0.take().unwrap())();
        }
    }

    struct MyFuture {
        _data: Box<Any>,
    }

    impl Future for MyFuture {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            Ok(().into())
        }
    }

    current_thread.spawn({
        MyFuture {
            _data: Box::new(OnDrop(Some(move || {
                current_thread::spawn(lazy(move || {
                    tx.send(()).unwrap();
                    Ok(())
                }));
            }))),
        }
    });

    current_thread.block_on(rx).unwrap();
    current_thread.run().unwrap();
}

#[test]
fn hammer_turn() {
    use futures::sync::mpsc;

    const ITER: usize = 100;
    const N: usize = 100;
    const THREADS: usize = 4;

    for _ in 0..ITER {
        let mut ths = vec![];

        // Add some jitter
        for _ in 0..THREADS {
            let th = thread::spawn(|| {
                let mut current_thread = CurrentThread::new();

                let (tx, rx) = mpsc::unbounded();

                current_thread.spawn({
                    rx.for_each(|_| {
                        Ok(())
                    })
                    .map_err(|e| panic!("err={:?}", e))
                });

                thread::spawn(move || {
                    for _ in 0..N {
                        tx.unbounded_send(()).unwrap();
                        thread::yield_now();
                    }
                });

                while !current_thread.is_idle() {
                    current_thread.turn(None).unwrap();
                }
            });

            ths.push(th);
        }

        for th in ths {
            th.join().unwrap();
        }
    }
}

fn ok() -> future::FutureResult<(), ()> {
    future::ok(())
}
