#![warn(rust_2018_idioms)]

use crate::executor::park::{Park, Unpark};
use crate::executor::thread_pool;

use futures_util::future::poll_fn;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::*;
use std::sync::{mpsc, Arc};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

#[test]
fn eagerly_drops_futures() {
    use std::sync::{mpsc, Mutex};

    struct MyPark {
        rx: mpsc::Receiver<()>,
        tx: Mutex<mpsc::Sender<()>>,
        #[allow(dead_code)]
        park_tx: mpsc::SyncSender<()>,
        unpark_tx: mpsc::SyncSender<()>,
    }

    impl Park for MyPark {
        type Unpark = MyUnpark;
        type Error = ();

        fn unpark(&self) -> Self::Unpark {
            MyUnpark {
                tx: Mutex::new(self.tx.lock().unwrap().clone()),
                unpark_tx: self.unpark_tx.clone(),
            }
        }

        fn park(&mut self) -> Result<(), Self::Error> {
            let _ = self.rx.recv();
            Ok(())
        }

        fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
            let _ = self.rx.recv_timeout(duration);
            Ok(())
        }
    }

    struct MyUnpark {
        tx: Mutex<mpsc::Sender<()>>,
        #[allow(dead_code)]
        unpark_tx: mpsc::SyncSender<()>,
    }

    impl Unpark for MyUnpark {
        fn unpark(&self) {
            let _ = self.tx.lock().unwrap().send(());
        }
    }

    let (task_tx, task_rx) = mpsc::channel();
    let (drop_tx, drop_rx) = mpsc::channel();
    let (park_tx, park_rx) = mpsc::sync_channel(0);
    let (unpark_tx, unpark_rx) = mpsc::sync_channel(0);

    let pool = thread_pool::Builder::new()
        .num_threads(4)
        .build_with_park(move |_| {
            let (tx, rx) = mpsc::channel();
            MyPark {
                tx: Mutex::new(tx),
                rx,
                park_tx: park_tx.clone(),
                unpark_tx: unpark_tx.clone(),
            }
        });

    struct MyTask {
        task_tx: Option<mpsc::Sender<Waker>>,
        drop_tx: mpsc::Sender<()>,
    }

    impl Future for MyTask {
        type Output = ();

        fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
            if let Some(tx) = self.get_mut().task_tx.take() {
                tx.send(cx.waker().clone()).unwrap();
            }

            Poll::Pending
        }
    }

    impl Drop for MyTask {
        fn drop(&mut self) {
            self.drop_tx.send(()).unwrap();
        }
    }

    pool.spawn(MyTask {
        task_tx: Some(task_tx),
        drop_tx,
    });

    // Wait until we get the task handle.
    let task = task_rx.recv().unwrap();

    // Drop the pool, this should result in futures being forcefully dropped.
    drop(pool);

    // Make sure `MyPark` and `MyUnpark` were dropped during shutdown.
    assert_eq!(park_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    assert_eq!(unpark_rx.try_recv(), Err(mpsc::TryRecvError::Disconnected));

    // If the future is forcefully dropped, then we will get a signal here.
    drop_rx.recv().unwrap();

    // Ensure `task` lives until after the test completes.
    drop(task);
}

#[test]
fn park_called_at_interval() {
    struct MyPark {
        park_light: Arc<AtomicBool>,
    }

    struct MyUnpark {}

    impl Park for MyPark {
        type Unpark = MyUnpark;
        type Error = ();

        fn unpark(&self) -> Self::Unpark {
            MyUnpark {}
        }

        fn park(&mut self) -> Result<(), Self::Error> {
            use std::thread;
            use std::time::Duration;

            thread::sleep(Duration::from_millis(1));
            Ok(())
        }

        fn park_timeout(&mut self, duration: Duration) -> Result<(), Self::Error> {
            if duration == Duration::from_millis(0) {
                self.park_light.store(true, Relaxed);
                Ok(())
            } else {
                self.park()
            }
        }
    }

    impl Unpark for MyUnpark {
        fn unpark(&self) {}
    }

    let park_light_1 = Arc::new(AtomicBool::new(false));
    let park_light_2 = park_light_1.clone();

    let (done_tx, done_rx) = mpsc::channel();

    // Use 1 thread to ensure the worker stays busy.
    let pool = thread_pool::Builder::new()
        .num_threads(1)
        .build_with_park(move |idx| {
            assert_eq!(idx, 0);
            MyPark {
                park_light: park_light_2.clone(),
            }
        });

    let mut cnt = 0;

    pool.spawn(poll_fn(move |cx| {
        let did_park_light = park_light_1.load(Relaxed);

        if did_park_light {
            // There is a bit of a race where the worker can tick a few times
            // before seeing the task
            assert!(cnt > 50);
            done_tx.send(()).unwrap();
            return Poll::Ready(());
        }

        cnt += 1;

        cx.waker().wake_by_ref();
        Poll::Pending
    }));

    done_rx.recv().unwrap();
}
