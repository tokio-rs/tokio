extern crate tokio_channel;
#[macro_use]
extern crate futures;

mod support;
use support::*;

use tokio_channel::mpsc;
use tokio_channel::oneshot;

use futures::prelude::*;
use futures::{future::lazy, task};

use std::thread;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};

trait AssertSend: Send {}
impl AssertSend for mpsc::Sender<i32> {}
impl AssertSend for mpsc::Receiver<i32> {}

#[test]
fn send_recv() {
    let (tx, rx) = mpsc::channel::<i32>(16);
    let mut rx = rx.wait();

    tx.send(1).wait().unwrap();

    assert_eq!(rx.next().unwrap(), Ok(1));
}

#[test]
fn send_recv_no_buffer() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(0);

    // Run on a task context
    lazy(move || {
        assert!(tx.poll_complete().unwrap().is_ready());
        assert!(tx.poll_ready().unwrap().is_ready());

        // Send first message
        let res = tx.start_send(1).unwrap();
        assert!(is_ready(&res));
        assert!(tx.poll_ready().unwrap().is_not_ready());

        // Send second message
        let res = tx.start_send(2).unwrap();
        assert!(!is_ready(&res));

        // Take the value
        assert_eq!(rx.poll().unwrap(), Async::Ready(Some(1)));
        assert!(tx.poll_ready().unwrap().is_ready());

        let res = tx.start_send(2).unwrap();
        assert!(is_ready(&res));
        assert!(tx.poll_ready().unwrap().is_not_ready());

        // Take the value
        assert_eq!(rx.poll().unwrap(), Async::Ready(Some(2)));
        assert!(tx.poll_ready().unwrap().is_ready());

        Ok::<(), ()>(())
    }).wait().unwrap();
}

#[test]
fn send_shared_recv() {
    let (tx1, rx) = mpsc::channel::<i32>(16);
    let tx2 = tx1.clone();
    let mut rx = rx.wait();

    tx1.send(1).wait().unwrap();
    assert_eq!(rx.next().unwrap(), Ok(1));

    tx2.send(2).wait().unwrap();
    assert_eq!(rx.next().unwrap(), Ok(2));
}

#[test]
fn send_recv_threads() {
    let (tx, rx) = mpsc::channel::<i32>(16);
    let mut rx = rx.wait();

    thread::spawn(move|| {
        tx.send(1).wait().unwrap();
    });

    assert_eq!(rx.next().unwrap(), Ok(1));
}

#[test]
fn send_recv_threads_no_capacity() {
    let (tx, rx) = mpsc::channel::<i32>(0);
    let mut rx = rx.wait();

    let (readytx, readyrx) = mpsc::channel::<()>(2);
    let mut readyrx = readyrx.wait();
    let t = thread::spawn(move|| {
        let readytx = readytx.sink_map_err(|_| panic!());
        let (a, b) = tx.send(1).join(readytx.send(())).wait().unwrap();
        a.send(2).join(b.send(())).wait().unwrap();
    });

    drop(readyrx.next().unwrap());
    assert_eq!(rx.next().unwrap(), Ok(1));
    drop(readyrx.next().unwrap());
    assert_eq!(rx.next().unwrap(), Ok(2));

    t.join().unwrap();
}

#[test]
fn recv_close_gets_none() {
    let (mut tx, mut rx) = mpsc::channel::<i32>(10);

    // Run on a task context
    lazy(move || {
        rx.close();

        assert_eq!(rx.poll(), Ok(Async::Ready(None)));
        assert!(tx.poll_ready().is_err());

        drop(tx);

        Ok::<(), ()>(())
    }).wait().unwrap();
}


#[test]
fn tx_close_gets_none() {
    let (_, mut rx) = mpsc::channel::<i32>(10);

    // Run on a task context
    lazy(move || {
        assert_eq!(rx.poll(), Ok(Async::Ready(None)));
        assert_eq!(rx.poll(), Ok(Async::Ready(None)));

        Ok::<(), ()>(())
    }).wait().unwrap();
}

#[test]
fn stress_shared_unbounded() {
    const AMT: u32 = 10000;
    const NTHREADS: u32 = 8;
    let (tx, rx) = mpsc::unbounded::<i32>();
    let mut rx = rx.wait();

    let t = thread::spawn(move|| {
        for _ in 0..AMT * NTHREADS {
            assert_eq!(rx.next().unwrap(), Ok(1));
        }

        if rx.next().is_some() {
            panic!();
        }
    });

    for _ in 0..NTHREADS {
        let tx = tx.clone();

        thread::spawn(move|| {
            for _ in 0..AMT {
                tx.unbounded_send(1).unwrap();
            }
        });
    }

    drop(tx);

    t.join().ok().unwrap();
}

#[test]
fn stress_shared_bounded_hard() {
    const AMT: u32 = 10000;
    const NTHREADS: u32 = 8;
    let (tx, rx) = mpsc::channel::<i32>(0);
    let mut rx = rx.wait();

    let t = thread::spawn(move|| {
        for _ in 0..AMT * NTHREADS {
            assert_eq!(rx.next().unwrap(), Ok(1));
        }

        if rx.next().is_some() {
            panic!();
        }
    });

    for _ in 0..NTHREADS {
        let mut tx = tx.clone();

        thread::spawn(move|| {
            for _ in 0..AMT {
                tx = tx.send(1).wait().unwrap();
            }
        });
    }

    drop(tx);

    t.join().ok().unwrap();
}

#[test]
fn stress_receiver_multi_task_bounded_hard() {
    const AMT: usize = 10_000;
    const NTHREADS: u32 = 2;

    let (mut tx, rx) = mpsc::channel::<usize>(0);
    let rx = Arc::new(Mutex::new(Some(rx)));
    let n = Arc::new(AtomicUsize::new(0));

    let mut th = vec![];

    for _ in 0..NTHREADS {
        let rx = rx.clone();
        let n = n.clone();

        let t = thread::spawn(move || {
            let mut i = 0;

            loop {
                i += 1;
                let mut lock = rx.lock().ok().unwrap();

                match lock.take() {
                    Some(mut rx) => {
                        if i % 5 == 0 {
                            let (item, rest) = rx.into_future().wait().ok().unwrap();

                            if item.is_none() {
                                break;
                            }

                            n.fetch_add(1, Ordering::Relaxed);
                            *lock = Some(rest);
                        } else {
                            // Just poll
                            let n = n.clone();
                            let r = lazy(move || {
                                let r = match rx.poll().unwrap() {
                                    Async::Ready(Some(_)) => {
                                        n.fetch_add(1, Ordering::Relaxed);
                                        *lock = Some(rx);
                                        false
                                    }
                                    Async::Ready(None) => {
                                        true
                                    }
                                    Async::NotReady => {
                                        *lock = Some(rx);
                                        false
                                    }
                                };

                                Ok::<bool, ()>(r)
                            }).wait().unwrap();

                            if r {
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
        });

        th.push(t);
    }

    for i in 0..AMT {
        tx = tx.send(i).wait().unwrap();
    }

    drop(tx);

    for t in th {
        t.join().unwrap();
    }

    assert_eq!(AMT, n.load(Ordering::Relaxed));
}

/// Stress test that receiver properly receives all the messages
/// after sender dropped.
#[test]
fn stress_drop_sender() {
    fn list() -> Box<Stream<Item=i32, Error=u32>> {
        let (tx, rx) = mpsc::channel(1);
        tx.send(Ok(1))
          .and_then(|tx| tx.send(Ok(2)))
          .and_then(|tx| tx.send(Ok(3)))
          .forget();
        Box::new(rx.then(|r| r.unwrap()))
    }

    for _ in 0..10000 {
        assert_eq!(list().wait().collect::<Result<Vec<_>, _>>(),
        Ok(vec![1, 2, 3]));
    }
}

/// Stress test that after receiver dropped,
/// no messages are lost.
fn stress_close_receiver_iter() {
    let (tx, rx) = mpsc::unbounded();
    let (unwritten_tx, unwritten_rx) = std::sync::mpsc::channel();
    let th = thread::spawn(move || {
        for i in 1.. {
            if let Err(_) = tx.unbounded_send(i) {
                unwritten_tx.send(i).expect("unwritten_tx");
                return;
            }
        }
    });

    let mut rx = rx.wait();

    // Read one message to make sure thread effectively started
    assert_eq!(Some(Ok(1)), rx.next());

    rx.get_mut().close();

    for i in 2.. {
        match rx.next() {
            Some(Ok(r)) => assert!(i == r),
            Some(Err(_)) => unreachable!(),
            None => {
                let unwritten = unwritten_rx.recv().expect("unwritten_rx");
                assert_eq!(unwritten, i);
                th.join().unwrap();
                return;
            }
        }
    }
}

#[test]
fn stress_close_receiver() {
    for _ in 0..10000 {
        stress_close_receiver_iter();
    }
}

/// Tests that after `poll_ready` indicates capacity a channel can always send without waiting.
#[test]
fn stress_poll_ready() {
    // A task which checks channel capacity using poll_ready, and pushes items onto the channel when
    // ready.
    struct SenderTask {
        sender: mpsc::Sender<u32>,
        count: u32,
    }
    impl Future for SenderTask {
        type Item = ();
        type Error = ();
        fn poll(&mut self) -> Poll<(), ()> {
            // In a loop, check if the channel is ready. If so, push an item onto the channel
            // (asserting that it doesn't attempt to block).
            while self.count > 0 {
                try_ready!(self.sender.poll_ready().map_err(|_| ()));
                assert!(self.sender.start_send(self.count).unwrap().is_ready());
                self.count -= 1;
            }
            Ok(Async::Ready(()))
        }
    }

    const AMT: u32 = 1000;
    const NTHREADS: u32 = 8;

    /// Run a stress test using the specified channel capacity.
    fn stress(capacity: usize) {
        let (tx, rx) = mpsc::channel(capacity);
        let mut threads = Vec::new();
        for _ in 0..NTHREADS {
            let sender = tx.clone();
            threads.push(thread::spawn(move || {
                SenderTask {
                    sender: sender,
                    count: AMT,
                }.wait()
            }));
        }
        drop(tx);

        let mut rx = rx.wait();
        for _ in 0..AMT * NTHREADS {
            assert!(rx.next().is_some());
        }

        assert!(rx.next().is_none());

        for thread in threads {
            thread.join().unwrap().unwrap();
        }
    }

    stress(0);
    stress(1);
    stress(8);
    stress(16);
}

// Stress test that `try_send()`s occurring concurrently with receiver
// close/drops don't appear as successful sends.
#[test]
fn stress_try_send_as_receiver_closes() {
    const AMT: usize = 10000;

    // To provide variable timing characteristics (in the hopes of
    // reproducing the collision that leads to a race), we busy-re-poll
    // the test MPSC receiver a variable number of times before actually
    // stopping.  We vary this countdown between 1 and the following
    // value.
    const MAX_COUNTDOWN: usize = 20;

    // When we detect that a successfully sent item is still in the
    // queue after a disconnect, we spin for up to 100ms to confirm that
    // it is a persistent condition and not a concurrency illusion.
    const SPIN_TIMEOUT: Duration = Duration::from_millis(100);

    struct TestRx {
        rx: mpsc::Receiver<Arc<()>>,
        // The number of times to query `rx` before dropping it.
        poll_count: usize
    }

    struct TestTask {
        command_rx: mpsc::Receiver<TestRx>,
        test_rx: Option<mpsc::Receiver<Arc<()>>>,
        countdown: usize,
    }

    impl TestTask {
        /// Create a new TestTask
        fn new() -> (TestTask, mpsc::Sender<TestRx>) {
            let (command_tx, command_rx) = mpsc::channel::<TestRx>(0);
            (
                TestTask {
                    command_rx,
                    test_rx: None,
                    countdown: 0, // 0 means no countdown is in progress.
                },
                command_tx,
            )
        }
    }

    impl Future for TestTask {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            // Poll the test channel, if one is present.
            if let Some(ref mut rx) = self.test_rx {
                if let Ok(Async::Ready(v)) = rx.poll() {
                   let _ = v.expect("test finished unexpectedly!");
                }
                self.countdown -= 1;
                // Busy-poll until the countdown is finished.
                task::current().notify();
            }

            // Accept any newly submitted MPSC channels for testing.
            match self.command_rx.poll()? {
                Async::Ready(Some(TestRx { rx, poll_count })) => {
                    self.test_rx = Some(rx);
                    self.countdown = poll_count;

                    task::current().notify();
                },
                Async::Ready(None) => return Ok(Async::Ready(())),
                _ => {},
            }

            if self.countdown == 0 {
                // Countdown complete -- drop the Receiver.
                self.test_rx = None;
            }

            Ok(Async::NotReady)
        }
    }

    let (f, mut cmd_tx) = TestTask::new();
    let bg = thread::spawn(move || f.wait());

    for i in 0..AMT {
        let (mut test_tx, rx) = mpsc::channel(0);
        let poll_count = i % MAX_COUNTDOWN;

        cmd_tx.try_send(TestRx { rx, poll_count }).unwrap();
        let mut prev_weak: Option<Weak<()>> = None;
        let mut attempted_sends = 0;
        let mut successful_sends = 0;
        loop {
            // Create a test item.
            let item = Arc::new(());
            let weak = Arc::downgrade(&item);

            match test_tx.try_send(item) {
                Ok(_) => {
                    prev_weak = Some(weak);
                    successful_sends += 1;
                }
                Err(ref e) if e.is_full() => {}
                Err(ref e) if e.is_disconnected() => {
                    // Test for evidence of the race condition.
                    if let Some(prev_weak) = prev_weak {
                        if prev_weak.upgrade().is_some() {
                            // The previously sent item is still allocated.
                            // However, there appears to be some aspect of the
                            // concurrency that can legitimately cause the Arc
                            // to be momentarily valid.  Spin for up to 100ms
                            // waiting for the previously sent item to be
                            // dropped.
                            let t0 = Instant::now();
                            let mut spins = 0;
                            loop {
                                if prev_weak.upgrade().is_none() {
                                    break;
                                }

                                assert!(t0.elapsed() < SPIN_TIMEOUT,
                                    "item not dropped on iteration {} after \
                                     {} sends ({} successful). spin=({})",
                                    i, attempted_sends, successful_sends, spins
                                );
                                spins += 1;
                            }
                        }
                    }
                    break;
                }
                Err(ref e) => panic!("unexpected error: {}", e),
            }
            attempted_sends += 1;
        }
    }

    drop(cmd_tx);

    bg.join()
        .expect("background thread join")
        .expect("background thread result");

}

fn is_ready<T>(res: &AsyncSink<T>) -> bool {
    match *res {
        AsyncSink::Ready => true,
        _ => false,
    }
}

#[test]
fn try_send_1() {
    const N: usize = 3000;
    let (mut tx, rx) = mpsc::channel(0);

    let t = thread::spawn(move || {
        for i in 0..N {
            loop {
                if tx.try_send(i).is_ok() {
                    break
                }
            }
        }
    });
    for (i, j) in rx.wait().enumerate() {
        assert_eq!(i, j.unwrap());
    }
    t.join().unwrap();
}

#[test]
fn try_send_2() {
    let (mut tx, rx) = mpsc::channel(0);

    tx.try_send("hello").unwrap();

    let (readytx, readyrx) = oneshot::channel::<()>();

    let th = thread::spawn(|| {
        lazy(|| {
            assert!(tx.start_send("fail").unwrap().is_not_ready());
            Ok::<_, ()>(())
        }).wait().unwrap();

        drop(readytx);
        tx.send("goodbye").wait().unwrap();
    });

    let mut rx = rx.wait();

    drop(readyrx.wait());
    assert_eq!(rx.next(), Some(Ok("hello")));
    assert_eq!(rx.next(), Some(Ok("goodbye")));
    assert!(rx.next().is_none());

    th.join().unwrap();
}

#[test]
fn try_send_fail() {
    let (mut tx, rx) = mpsc::channel(0);
    let mut rx = rx.wait();

    tx.try_send("hello").unwrap();

    // This should fail
    assert!(tx.try_send("fail").is_err());

    assert_eq!(rx.next(), Some(Ok("hello")));

    tx.try_send("goodbye").unwrap();
    drop(tx);

    assert_eq!(rx.next(), Some(Ok("goodbye")));
    assert!(rx.next().is_none());
}
