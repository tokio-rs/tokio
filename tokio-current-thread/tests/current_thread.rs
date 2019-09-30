extern crate futures;
extern crate tokio_current_thread;
extern crate tokio_executor;

use tokio_current_thread::{block_on_all, CurrentThread};

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::thread;
use std::time::Duration;

use futures::future::{self, lazy};
use futures::task;
// This is not actually unused --- we need this trait to be in scope for
// the tests that sue TaskExecutor::current().execute(). The compiler
// doesn't realise that.
#[allow(unused_imports)]
use futures::future::Executor as _futures_Executor;
use futures::prelude::*;
use futures::sync::oneshot;

mod from_block_on_all {
    use super::*;
    fn test<F: Fn(Box<dyn Future<Item = (), Error = ()>>) + 'static>(spawn: F) {
        let cnt = Rc::new(Cell::new(0));
        let c = cnt.clone();

        let msg = tokio_current_thread::block_on_all(lazy(move || {
            c.set(1 + c.get());

            // Spawn!
            spawn(Box::new(lazy(move || {
                c.set(1 + c.get());
                Ok::<(), ()>(())
            })));

            Ok::<_, ()>("hello")
        }))
        .unwrap();

        assert_eq!(2, cnt.get());
        assert_eq!(msg, "hello");
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn)
    }

    #[test]
    fn execute() {
        test(|f| {
            tokio_current_thread::TaskExecutor::current()
                .execute(f)
                .unwrap();
        });
    }
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
    }))
    .unwrap();

    assert_eq!(1, cnt2.get());
}

#[test]
fn spawn_many() {
    const ITER: usize = 200;

    let cnt = Rc::new(Cell::new(0));
    let mut tokio_current_thread = CurrentThread::new();

    for _ in 0..ITER {
        let cnt = cnt.clone();
        tokio_current_thread.spawn(lazy(move || {
            cnt.set(1 + cnt.get());
            Ok::<(), ()>(())
        }));
    }

    tokio_current_thread.run().unwrap();

    assert_eq!(cnt.get(), ITER);
}

mod does_not_set_global_executor_by_default {
    use super::*;

    fn test<F: Fn(Box<dyn Future<Item = (), Error = ()> + Send>) -> Result<(), E> + 'static, E>(
        spawn: F,
    ) {
        block_on_all(lazy(|| {
            spawn(Box::new(lazy(|| ok()))).unwrap_err();
            ok()
        }))
        .unwrap()
    }

    #[test]
    fn spawn() {
        use tokio_executor::Executor;
        test(|f| tokio_executor::DefaultExecutor::current().spawn(f))
    }

    #[test]
    fn execute() {
        test(|f| tokio_executor::DefaultExecutor::current().execute(f))
    }
}

mod from_block_on_future {
    use super::*;

    fn test<F: Fn(Box<dyn Future<Item = (), Error = ()>>)>(spawn: F) {
        let cnt = Rc::new(Cell::new(0));

        let mut tokio_current_thread = CurrentThread::new();

        tokio_current_thread
            .block_on(lazy(|| {
                let cnt = cnt.clone();

                spawn(Box::new(lazy(move || {
                    cnt.set(1 + cnt.get());
                    Ok(())
                })));

                Ok::<_, ()>(())
            }))
            .unwrap();

        tokio_current_thread.run().unwrap();

        assert_eq!(1, cnt.get());
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn);
    }

    #[test]
    fn execute() {
        test(|f| {
            tokio_current_thread::TaskExecutor::current()
                .execute(f)
                .unwrap();
        });
    }
}

struct Never(Rc<()>);

impl Future for Never {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        Ok(Async::NotReady)
    }
}

mod outstanding_tasks_are_dropped_when_executor_is_dropped {
    use super::*;

    fn test<F, G>(spawn: F, dotspawn: G)
    where
        F: Fn(Box<dyn Future<Item = (), Error = ()>>) + 'static,
        G: Fn(&mut CurrentThread, Box<dyn Future<Item = (), Error = ()>>),
    {
        let mut rc = Rc::new(());

        let mut tokio_current_thread = CurrentThread::new();
        dotspawn(&mut tokio_current_thread, Box::new(Never(rc.clone())));

        drop(tokio_current_thread);

        // Ensure the daemon is dropped
        assert!(Rc::get_mut(&mut rc).is_some());

        // Using the global spawn fn

        let mut rc = Rc::new(());

        let mut tokio_current_thread = CurrentThread::new();

        tokio_current_thread
            .block_on(lazy(|| {
                spawn(Box::new(Never(rc.clone())));
                Ok::<_, ()>(())
            }))
            .unwrap();

        drop(tokio_current_thread);

        // Ensure the daemon is dropped
        assert!(Rc::get_mut(&mut rc).is_some());
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn, |rt, f| {
            rt.spawn(f);
        })
    }

    #[test]
    fn execute() {
        test(
            |f| {
                tokio_current_thread::TaskExecutor::current()
                    .execute(f)
                    .unwrap();
            },
            // Note: `CurrentThread` doesn't currently implement
            // `futures::Executor`, so we'll call `.spawn(...)` rather than
            // `.execute(...)` for now. If `CurrentThread` is changed to
            // implement Executor, change this to `.execute(...).unwrap()`.
            |rt, f| {
                rt.spawn(f);
            },
        );
    }
}

#[test]
#[should_panic]
fn nesting_run() {
    block_on_all(lazy(|| {
        block_on_all(lazy(|| ok())).unwrap();

        ok()
    }))
    .unwrap();
}

mod run_in_future {
    use super::*;

    #[test]
    #[should_panic]
    fn spawn() {
        block_on_all(lazy(|| {
            tokio_current_thread::spawn(lazy(|| {
                block_on_all(lazy(|| ok())).unwrap();
                ok()
            }));
            ok()
        }))
        .unwrap();
    }

    #[test]
    #[should_panic]
    fn execute() {
        block_on_all(lazy(|| {
            tokio_current_thread::TaskExecutor::current()
                .execute(lazy(|| {
                    block_on_all(lazy(|| ok())).unwrap();
                    ok()
                }))
                .unwrap();
            ok()
        }))
        .unwrap();
    }
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
        .spawn(Infini { num: num.clone() })
        .turn(None)
        .unwrap();

    assert_eq!(1, num.get());
}

mod tasks_are_scheduled_fairly {
    use super::*;
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

    fn test<F: Fn(Spin)>(spawn: F) {
        let state = Rc::new(RefCell::new([0, 0]));

        block_on_all(lazy(|| {
            spawn(Spin {
                state: state.clone(),
                idx: 0,
            });

            spawn(Spin {
                state: state,
                idx: 1,
            });

            ok()
        }))
        .unwrap();
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn)
    }

    #[test]
    fn execute() {
        test(|f| {
            tokio_current_thread::TaskExecutor::current()
                .execute(f)
                .unwrap();
        })
    }
}

mod and_turn {
    use super::*;

    fn test<F, G>(spawn: F, dotspawn: G)
    where
        F: Fn(Box<dyn Future<Item = (), Error = ()>>) + 'static,
        G: Fn(&mut CurrentThread, Box<dyn Future<Item = (), Error = ()>>),
    {
        let cnt = Rc::new(Cell::new(0));
        let c = cnt.clone();

        let mut tokio_current_thread = CurrentThread::new();

        // Spawn a basic task to get the executor to turn
        dotspawn(&mut tokio_current_thread, Box::new(lazy(move || Ok(()))));

        // Turn once...
        tokio_current_thread.turn(None).unwrap();

        dotspawn(
            &mut tokio_current_thread,
            Box::new(lazy(move || {
                c.set(1 + c.get());

                // Spawn!
                spawn(Box::new(lazy(move || {
                    c.set(1 + c.get());
                    Ok::<(), ()>(())
                })));

                Ok(())
            })),
        );

        // This does not run the newly spawned thread
        tokio_current_thread.turn(None).unwrap();
        assert_eq!(1, cnt.get());

        // This runs the newly spawned thread
        tokio_current_thread.turn(None).unwrap();
        assert_eq!(2, cnt.get());
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn, |rt, f| {
            rt.spawn(f);
        })
    }

    #[test]
    fn execute() {
        test(
            |f| {
                tokio_current_thread::TaskExecutor::current()
                    .execute(f)
                    .unwrap();
            },
            // Note: `CurrentThread` doesn't currently implement
            // `futures::Executor`, so we'll call `.spawn(...)` rather than
            // `.execute(...)` for now. If `CurrentThread` is changed to
            // implement Executor, change this to `.execute(...).unwrap()`.
            |rt, f| {
                rt.spawn(f);
            },
        );
    }
}

mod in_drop {
    use super::*;
    struct OnDrop<F: FnOnce()>(Option<F>);

    impl<F: FnOnce()> Drop for OnDrop<F> {
        fn drop(&mut self) {
            (self.0.take().unwrap())();
        }
    }

    struct MyFuture {
        _data: Box<dyn Any>,
    }

    impl Future for MyFuture {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            Ok(().into())
        }
    }

    fn test<F, G>(spawn: F, dotspawn: G)
    where
        F: Fn(Box<dyn Future<Item = (), Error = ()>>) + 'static,
        G: Fn(&mut CurrentThread, Box<dyn Future<Item = (), Error = ()>>),
    {
        let mut tokio_current_thread = CurrentThread::new();

        let (tx, rx) = oneshot::channel();

        dotspawn(
            &mut tokio_current_thread,
            Box::new(MyFuture {
                _data: Box::new(OnDrop(Some(move || {
                    spawn(Box::new(lazy(move || {
                        tx.send(()).unwrap();
                        Ok(())
                    })));
                }))),
            }),
        );

        tokio_current_thread.block_on(rx).unwrap();
        tokio_current_thread.run().unwrap();
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn, |rt, f| {
            rt.spawn(f);
        })
    }

    #[test]
    fn execute() {
        test(
            |f| {
                tokio_current_thread::TaskExecutor::current()
                    .execute(f)
                    .unwrap();
            },
            // Note: `CurrentThread` doesn't currently implement
            // `futures::Executor`, so we'll call `.spawn(...)` rather than
            // `.execute(...)` for now. If `CurrentThread` is changed to
            // implement Executor, change this to `.execute(...).unwrap()`.
            |rt, f| {
                rt.spawn(f);
            },
        );
    }
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
                let mut tokio_current_thread = CurrentThread::new();

                let (tx, rx) = mpsc::unbounded();

                tokio_current_thread.spawn({
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

                while !tokio_current_thread.is_idle() {
                    tokio_current_thread.turn(None).unwrap();
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
    let mut tokio_current_thread = CurrentThread::new();

    // Spawn oneshot receiver
    let (sender, receiver) = oneshot::channel::<()>();
    tokio_current_thread.spawn(receiver.then(|_| Ok(())));

    // Turn once...
    let res = tokio_current_thread
        .turn(Some(Duration::from_millis(0)))
        .unwrap();

    // Should've polled the receiver once, but considered it not ready
    assert!(res.has_polled());

    // Turn another time
    let res = tokio_current_thread
        .turn(Some(Duration::from_millis(0)))
        .unwrap();

    // Should've polled nothing, the receiver is not ready yet
    assert!(!res.has_polled());

    // Make the receiver ready
    sender.send(()).unwrap();

    // Turn another time
    let res = tokio_current_thread
        .turn(Some(Duration::from_millis(0)))
        .unwrap();

    // Should've polled the receiver, it's ready now
    assert!(res.has_polled());

    // Now the executor should be empty
    assert!(tokio_current_thread.is_idle());
    let res = tokio_current_thread
        .turn(Some(Duration::from_millis(0)))
        .unwrap();

    // So should've polled nothing
    assert!(!res.has_polled());
}

// Our own mock Park that is never really waiting and the only
// thing it does is to send, on request, something (once) to a oneshot
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

    let mut tokio_current_thread = CurrentThread::new_with_park(my_park);

    let receiver_1_done = Rc::new(Cell::new(false));
    let receiver_1_done_clone = receiver_1_done.clone();

    // Once an item is received on the oneshot channel, it will immediately
    // immediately make the second oneshot channel ready
    tokio_current_thread.spawn(receiver.map_err(|_| unreachable!()).and_then(move |_| {
        sender_2.send(()).unwrap();
        receiver_1_done_clone.set(true);

        Ok(())
    }));

    let receiver_2_done = Rc::new(Cell::new(false));
    let receiver_2_done_clone = receiver_2_done.clone();

    tokio_current_thread.spawn(receiver_2.map_err(|_| unreachable!()).and_then(move |_| {
        receiver_2_done_clone.set(true);
        Ok(())
    }));

    // The third receiver is only woken up from our Park implementation, it simulates
    // e.g. a socket that first has to be polled to know if it is ready now
    let receiver_3_done = Rc::new(Cell::new(false));
    let receiver_3_done_clone = receiver_3_done.clone();

    tokio_current_thread.spawn(receiver_3.map_err(|_| unreachable!()).and_then(move |_| {
        receiver_3_done_clone.set(true);
        Ok(())
    }));

    // First turn should've polled both and considered them not ready
    let res = tokio_current_thread
        .turn(Some(Duration::from_millis(0)))
        .unwrap();
    assert!(res.has_polled());

    // Next turn should've polled nothing
    let res = tokio_current_thread
        .turn(Some(Duration::from_millis(0)))
        .unwrap();
    assert!(!res.has_polled());

    assert!(!receiver_1_done.get());
    assert!(!receiver_2_done.get());
    assert!(!receiver_3_done.get());

    // After this the receiver future will wake up the second receiver future,
    // so there are pending futures again
    sender.send(()).unwrap();

    // Now the first receiver should be done, the second receiver should be ready
    // to be polled again and the socket not yet
    let res = tokio_current_thread.turn(None).unwrap();
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
    let res = tokio_current_thread.turn(None).unwrap();
    assert!(res.has_polled());

    assert!(receiver_1_done.get());
    assert!(receiver_2_done.get());
    assert!(receiver_3_done.get());

    // Don't send again
    send_now.set(false);

    // Now we should be idle and turning should not poll anything
    assert!(tokio_current_thread.is_idle());
    let res = tokio_current_thread.turn(None).unwrap();
    assert!(!res.has_polled());
}

#[test]
fn spawn_from_other_thread() {
    let mut current_thread = CurrentThread::new();

    let handle = current_thread.handle();
    let (sender, receiver) = oneshot::channel::<()>();

    thread::spawn(move || {
        handle
            .spawn(lazy(move || {
                sender.send(()).unwrap();
                Ok(())
            }))
            .unwrap();
    });

    let _ = current_thread.block_on(receiver).unwrap();
}

#[test]
fn spawn_from_other_thread_unpark() {
    use std::sync::mpsc::channel as mpsc_channel;

    let mut current_thread = CurrentThread::new();

    let handle = current_thread.handle();
    let (sender_1, receiver_1) = oneshot::channel::<()>();
    let (sender_2, receiver_2) = mpsc_channel::<()>();

    thread::spawn(move || {
        let _ = receiver_2.recv().unwrap();

        handle
            .spawn(lazy(move || {
                sender_1.send(()).unwrap();
                Ok(())
            }))
            .unwrap();
    });

    // Ensure that unparking the executor works correctly. It will first
    // check if there are new futures (there are none), then execute the
    // lazy future below which will cause the future to be spawned from
    // the other thread. Then the executor will park but should be woken
    // up because *now* we have a new future to schedule
    let _ = current_thread
        .block_on(
            lazy(move || {
                sender_2.send(()).unwrap();
                Ok(())
            })
            .and_then(|_| receiver_1),
        )
        .unwrap();
}

#[test]
fn spawn_from_executor_with_handle() {
    let mut current_thread = CurrentThread::new();
    let handle = current_thread.handle();
    let (tx, rx) = oneshot::channel();

    current_thread.spawn(lazy(move || {
        handle
            .spawn(lazy(move || {
                tx.send(()).unwrap();
                Ok(())
            }))
            .unwrap();
        Ok::<_, ()>(())
    }));

    current_thread.run();

    rx.wait().unwrap();
}

fn ok() -> future::FutureResult<(), ()> {
    future::ok(())
}
