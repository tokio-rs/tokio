#![cfg(not(feature = "unstable-futures"))]

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
    let (tx, rx) = oneshot::channel();

    thread::spawn(|| {
        thread::sleep(Duration::from_millis(1000));
        tx.send(()).unwrap();
    });

    let cnt = Rc::new(Cell::new(0));
    let cnt2 = cnt.clone();

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

    current_thread.spawn({
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
                    let cnt = Rc::new(Cell::new(0));
                    let c = cnt.clone();

                    rx.for_each(move |_| {
                        c.set(1 + c.get());
                        Ok(())
                    })
                    .map_err(|e| panic!("err={:?}", e))
                    .map(move |v| {
                        assert_eq!(N, cnt.get());
                        v
                    })
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

#[test]
fn turn_has_polled() {
    let mut current_thread = CurrentThread::new();

    // Spawn oneshot receiver
    let (sender, receiver) = oneshot::channel::<()>();
    current_thread.spawn(receiver.then(|_| Ok(())));

    // Turn once...
    let res = current_thread.turn(Some(Duration::from_millis(0))).unwrap();

    // Should've polled the receiver once, but considered it not ready
    assert!(res.has_polled());

    // Turn another time
    let res = current_thread.turn(Some(Duration::from_millis(0))).unwrap();

    // Should've polled nothing, the receiver is not ready yet
    assert!(!res.has_polled());

    // Make the receiver ready
    sender.send(()).unwrap();

    // Turn another time
    let res = current_thread.turn(Some(Duration::from_millis(0))).unwrap();

    // Should've polled the receiver, it's ready now
    assert!(res.has_polled());

    // Now the executor should be empty
    assert!(current_thread.is_idle());
    let res = current_thread.turn(Some(Duration::from_millis(0))).unwrap();

    // So should've polled nothing
    assert!(!res.has_polled());
}

// Our own mock Park that is never really waiting and the only
// thing it does is to send, on request, something (once) to a onshot
// channel
struct MyPark {
    sender: Option<oneshot::Sender<()>>,
    send_now: Rc<Cell<bool>>,
}

struct MyUnpark;

impl tokio_executor::park::Park for MyPark {
    type Unpark = MyUnpark;
    type Error = ();

    fn unpark(&self) -> Self::Unpark {
        MyUnpark
    }

    fn park(&mut self) -> Result<(), Self::Error> {
        // If called twice with send_now, this will intentionally panic
        if self.send_now.get() {
            self.sender.take().unwrap().send(()).unwrap();
        }

        Ok(())
    }

    fn park_timeout(&mut self, _duration: Duration) -> Result<(), Self::Error> {
        self.park()
    }
}

impl tokio_executor::park::Unpark for MyUnpark {
    fn unpark(&self) {}
}

#[test]
fn turn_fair() {
    let send_now = Rc::new(Cell::new(false));

    let (sender, receiver) = oneshot::channel::<()>();
    let (sender_2, receiver_2) = oneshot::channel::<()>();
    let (sender_3, receiver_3) = oneshot::channel::<()>();

    let my_park = MyPark {
        sender: Some(sender_3),
        send_now: send_now.clone(),
    };

    let mut current_thread = CurrentThread::new_with_park(my_park);

    let receiver_1_done = Rc::new(Cell::new(false));
    let receiver_1_done_clone = receiver_1_done.clone();

    // Once an item is received on the oneshot channel, it will immediately
    // immediately make the second oneshot channel ready
    current_thread.spawn(receiver
        .map_err(|_| unreachable!())
        .and_then(move |_| {
            sender_2.send(()).unwrap();
            receiver_1_done_clone.set(true);

            Ok(())
        })
    );

    let receiver_2_done = Rc::new(Cell::new(false));
    let receiver_2_done_clone = receiver_2_done.clone();

    current_thread.spawn(receiver_2
        .map_err(|_| unreachable!())
        .and_then(move |_| {
            receiver_2_done_clone.set(true);
            Ok(())
        })
    );

    // The third receiver is only woken up from our Park implementation, it simulates
    // e.g. a socket that first has to be polled to know if it is ready now
    let receiver_3_done = Rc::new(Cell::new(false));
    let receiver_3_done_clone = receiver_3_done.clone();

    current_thread.spawn(receiver_3
        .map_err(|_| unreachable!())
        .and_then(move |_| {
            receiver_3_done_clone.set(true);
            Ok(())
        })
    );

    // First turn should've polled both and considered them not ready
    let res = current_thread.turn(Some(Duration::from_millis(0))).unwrap();
    assert!(res.has_polled());

    // Next turn should've polled nothing
    let res = current_thread.turn(Some(Duration::from_millis(0))).unwrap();
    assert!(!res.has_polled());

    assert!(!receiver_1_done.get());
    assert!(!receiver_2_done.get());
    assert!(!receiver_3_done.get());

    // After this the receiver future will wake up the second receiver future,
    // so there are pending futures again
    sender.send(()).unwrap();

    // Now the first receiver should be done, the second receiver should be ready
    // to be polled again and the socket not yet
    let res = current_thread.turn(None).unwrap();
    assert!(res.has_polled());

    assert!(receiver_1_done.get());
    assert!(!receiver_2_done.get());
    assert!(!receiver_3_done.get());

    // Now let our park implementation know that it should send something to sender 3
    send_now.set(true);

    // This should resolve the second receiver directly, but also poll the socket
    // and read the packet from it. If it didn't do both here, we would handle
    // futures that are woken up from the reactor and directly unfairly and would
    // favour the ones that are woken up directly.
    let res = current_thread.turn(None).unwrap();
    assert!(res.has_polled());

    assert!(receiver_1_done.get());
    assert!(receiver_2_done.get());
    assert!(receiver_3_done.get());

    // Don't send again
    send_now.set(false);

    // Now we should be idle and turning should not poll anything
    assert!(current_thread.is_idle());
    let res = current_thread.turn(None).unwrap();
    assert!(!res.has_polled());
}

fn ok() -> future::FutureResult<(), ()> {
    future::ok(())
}
