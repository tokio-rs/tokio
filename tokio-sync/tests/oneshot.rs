extern crate futures;
extern crate tokio_mock_task;
extern crate tokio_sync;

use tokio_sync::oneshot;
use tokio_mock_task::*;

use futures::prelude::*;

macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }}
}

macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(futures::Async::NotReady) => {},
            Ok(futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }}
}

#[test]
fn send_recv() {
    let (tx, mut rx) = oneshot::channel();
    let mut task = MockTask::new();

    task.enter(|| {
        assert_not_ready!(rx.poll());
    });

    tx.send(1).unwrap();

    assert!(task.is_notified());

    let val = assert_ready!(rx.poll());
    assert_eq!(val, 1);
}
