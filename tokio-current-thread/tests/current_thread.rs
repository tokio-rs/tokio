#![deny(warnings, rust_2018_idioms)]
#![feature(async_await)]

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};
use std::thread;
use std::time::Duration;
use tokio_current_thread::{block_on_all, CurrentThread};
use tokio_executor::TypedExecutor;
use tokio_sync::oneshot;

mod from_block_on_all {
    use super::*;
    fn test<F: Fn(Pin<Box<dyn Future<Output = ()>>>) + 'static>(spawn: F) {
        let cnt = Rc::new(Cell::new(0));
        let c = cnt.clone();

        let msg = tokio_current_thread::block_on_all(async move {
            c.set(1 + c.get());

            // Spawn!
            spawn(Box::pin(async move {
                c.set(1 + c.get());
            }));

            "hello"
        });

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
                .spawn(f)
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

    block_on_all(async move {
        rx.await.unwrap();
        cnt.set(1 + cnt.get());
    });

    assert_eq!(1, cnt2.get());
}

#[test]
fn spawn_many() {
    const ITER: usize = 200;

    let cnt = Rc::new(Cell::new(0));
    let mut tokio_current_thread = CurrentThread::new();

    for _ in 0..ITER {
        let cnt = cnt.clone();
        tokio_current_thread.spawn(async move {
            cnt.set(1 + cnt.get());
        });
    }

    tokio_current_thread.run().unwrap();

    assert_eq!(cnt.get(), ITER);
}

mod does_not_set_global_executor_by_default {
    use super::*;

    fn test<F: Fn(Pin<Box<dyn Future<Output = ()> + Send>>) -> Result<(), E> + 'static, E>(
        spawn: F,
    ) {
        block_on_all(async {
            spawn(Box::pin(async {})).unwrap_err();
        });
    }

    #[test]
    fn spawn() {
        test(|f| tokio_executor::DefaultExecutor::current().spawn(f))
    }
}

mod from_block_on_future {
    use super::*;

    fn test<F: Fn(Pin<Box<dyn Future<Output = ()>>>)>(spawn: F) {
        let cnt = Rc::new(Cell::new(0));
        let cnt2 = cnt.clone();

        let mut tokio_current_thread = CurrentThread::new();

        tokio_current_thread.block_on(async move {
            let cnt3 = cnt2.clone();

            spawn(Box::pin(async move {
                cnt3.set(1 + cnt3.get());
            }));
        });

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
                .spawn(f)
                .unwrap();
        });
    }
}

mod outstanding_tasks_are_dropped_when_executor_is_dropped {
    use super::*;

    async fn never(_rc: Rc<()>) {
        loop {
            yield_once().await;
        }
    }

    fn test<F, G>(spawn: F, dotspawn: G)
    where
        F: Fn(Pin<Box<dyn Future<Output = ()>>>) + 'static,
        G: Fn(&mut CurrentThread, Pin<Box<dyn Future<Output = ()>>>),
    {
        let mut rc = Rc::new(());

        let mut tokio_current_thread = CurrentThread::new();
        dotspawn(&mut tokio_current_thread, Box::pin(never(rc.clone())));

        drop(tokio_current_thread);

        // Ensure the daemon is dropped
        assert!(Rc::get_mut(&mut rc).is_some());

        // Using the global spawn fn

        let mut rc = Rc::new(());
        let rc2 = rc.clone();

        let mut tokio_current_thread = CurrentThread::new();

        tokio_current_thread.block_on(async move {
            spawn(Box::pin(never(rc2)));
        });

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
                    .spawn(f)
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
    block_on_all(async {
        block_on_all(async {});
    });
}

mod run_in_future {
    use super::*;

    #[test]
    #[should_panic]
    fn spawn() {
        block_on_all(async {
            tokio_current_thread::spawn(async {
                block_on_all(async {});
            });
        });
    }

    #[test]
    #[should_panic]
    fn execute() {
        block_on_all(async {
            tokio_current_thread::TaskExecutor::current()
                .spawn(async {
                    block_on_all(async {});
                })
                .unwrap();
        });
    }
}

#[test]
fn tick_on_infini_future() {
    let num = Rc::new(Cell::new(0));

    async fn infini(num: Rc<Cell<usize>>) {
        loop {
            num.set(1 + num.get());
            yield_once().await
        }
    }

    CurrentThread::new()
        .spawn(infini(num.clone()))
        .turn(None)
        .unwrap();

    assert_eq!(1, num.get());
}

mod tasks_are_scheduled_fairly {
    use super::*;

    async fn spin(state: Rc<RefCell<[i32; 2]>>, idx: usize) {
        loop {
            // borrow_mut scope
            {
                let mut state = state.borrow_mut();

                if idx == 0 {
                    let diff = state[0] - state[1];

                    assert!(diff.abs() <= 1);

                    if state[0] >= 50 {
                        return;
                    }
                }

                state[idx] += 1;

                if state[idx] >= 100 {
                    return;
                }
            }

            yield_once().await;
        }
    }

    fn test<F: Fn(Pin<Box<dyn Future<Output = ()>>>)>(spawn: F) {
        let state = Rc::new(RefCell::new([0, 0]));

        block_on_all(async move {
            spawn(Box::pin(spin(state.clone(), 0)));
            spawn(Box::pin(spin(state, 1)));
        });
    }

    #[test]
    fn spawn() {
        test(tokio_current_thread::spawn)
    }

    #[test]
    fn execute() {
        test(|f| {
            tokio_current_thread::TaskExecutor::current()
                .spawn(f)
                .unwrap();
        })
    }
}

mod and_turn {
    use super::*;

    fn test<F, G>(spawn: F, dotspawn: G)
    where
        F: Fn(Pin<Box<dyn Future<Output = ()>>>) + 'static,
        G: Fn(&mut CurrentThread, Pin<Box<dyn Future<Output = ()>>>),
    {
        let cnt = Rc::new(Cell::new(0));
        let c = cnt.clone();

        let mut tokio_current_thread = CurrentThread::new();

        // Spawn a basic task to get the executor to turn
        dotspawn(&mut tokio_current_thread, Box::pin(async {}));

        // Turn once...
        tokio_current_thread.turn(None).unwrap();

        dotspawn(
            &mut tokio_current_thread,
            Box::pin(async move {
                c.set(1 + c.get());

                // Spawn!
                spawn(Box::pin(async move {
                    c.set(1 + c.get());
                }));
            }),
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
                    .spawn(f)
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

    async fn noop(_data: Box<dyn Any>) {}

    fn test<F, G>(spawn: F, dotspawn: G)
    where
        F: Fn(Pin<Box<dyn Future<Output = ()>>>) + 'static,
        G: Fn(&mut CurrentThread, Pin<Box<dyn Future<Output = ()>>>),
    {
        let mut tokio_current_thread = CurrentThread::new();

        let (tx, rx) = oneshot::channel();

        dotspawn(
            &mut tokio_current_thread,
            Box::pin(noop(Box::new(OnDrop(Some(move || {
                spawn(Box::pin(async move {
                    tx.send(()).unwrap();
                }));
            }))))),
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
                    .spawn(f)
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

/*
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
*/

#[test]
fn turn_has_polled() {
    let mut tokio_current_thread = CurrentThread::new();

    // Spawn oneshot receiver
    let (sender, receiver) = oneshot::channel::<()>();
    tokio_current_thread.spawn(async move {
        let _ = receiver.await;
    });

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

    tokio_current_thread.spawn(async move {
        receiver.await.unwrap();
        sender_2.send(()).unwrap();
        receiver_1_done_clone.set(true);
    });

    let receiver_2_done = Rc::new(Cell::new(false));
    let receiver_2_done_clone = receiver_2_done.clone();

    tokio_current_thread.spawn(async move {
        receiver_2.await.unwrap();
        receiver_2_done_clone.set(true);
    });

    // The third receiver is only woken up from our Park implementation, it simulates
    // e.g. a socket that first has to be polled to know if it is ready now
    let receiver_3_done = Rc::new(Cell::new(false));
    let receiver_3_done_clone = receiver_3_done.clone();

    tokio_current_thread.spawn(async move {
        receiver_3.await.unwrap();
        receiver_3_done_clone.set(true);
    });

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
            .spawn(async move {
                sender.send(()).unwrap();
            })
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
            .spawn(async move {
                sender_1.send(()).unwrap();
            })
            .unwrap();
    });

    // Ensure that unparking the executor works correctly. It will first
    // check if there are new futures (there are none), then execute the
    // lazy future below which will cause the future to be spawned from
    // the other thread. Then the executor will park but should be woken
    // up because *now* we have a new future to schedule
    let _ = current_thread.block_on(async move {
        // inlined 'lazy'
        async move {
            sender_2.send(()).unwrap();
        }
            .await;
        receiver_1.await.unwrap();
    });
}

#[test]
fn spawn_from_executor_with_handle() {
    let mut current_thread = CurrentThread::new();
    let handle = current_thread.handle();
    let (tx, rx) = oneshot::channel();

    current_thread.spawn(async move {
        handle
            .spawn(async move {
                tx.send(()).unwrap();
            })
            .unwrap();
    });

    current_thread.block_on(rx).unwrap();
}

#[test]
fn handle_status() {
    let current_thread = CurrentThread::new();
    let handle = current_thread.handle();
    assert!(handle.status().is_ok());

    drop(current_thread);
    assert!(handle.spawn(async { () }).is_err());
    assert!(handle.status().is_err());
}

#[test]
fn handle_is_sync() {
    let current_thread = CurrentThread::new();
    let handle = current_thread.handle();

    let _box: Box<dyn Sync> = Box::new(handle);
}

async fn yield_once() {
    YieldOnce(false).await
}

struct YieldOnce(bool);

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            // Push to the back of the executor's queue
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
