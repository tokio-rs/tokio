extern crate futures;
extern crate tokio_executor;
extern crate tokio_reactor;
extern crate tokio_tcp;

use tokio_reactor::Reactor;
use tokio_tcp::TcpListener;

use futures::{Future, Stream};
use futures::executor::{spawn, Notify, Spawn};

use std::mem;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

#[test]
fn test_drop_on_notify() {
    // When the reactor receives a kernel notification, it notifies the
    // task that holds the associated socket. If this notification results in
    // the task being dropped, the socket will also be dropped.
    //
    // Previously, there was a deadlock scenario where the reactor, while
    // notifying, held a lock and the task being dropped attempted to acquire
    // that same lock in order to clean up state.
    //
    // To simulate this case, we create a fake executor that does nothing when
    // the task is notified. This simulates an executor in the process of
    // shutting down. Then, when the task handle is dropped, the task itself is
    // dropped.

    struct MyNotify;

    type Task = Mutex<Spawn<Box<Future<Item = (), Error = ()>>>>;

    impl Notify for MyNotify {
        fn notify(&self, _: usize) {
            // Do nothing
        }

        fn clone_id(&self, id: usize) -> usize {
            let ptr = id as *const Task;
            let task = unsafe { Arc::from_raw(ptr) };

            mem::forget(task.clone());
            mem::forget(task);

            id
        }

        fn drop_id(&self, id: usize) {
            let ptr = id as *const Task;
            let _ = unsafe { Arc::from_raw(ptr) };
        }
    }

    let addr = "127.0.0.1:0".parse().unwrap();
    let mut reactor = Reactor::new().unwrap();

    // Create a listener
    let listener = TcpListener::bind(&addr).unwrap();
    let addr = listener.local_addr().unwrap();

    // Define a task that just drains the listener
    let task = Box::new({
        listener.incoming()
            .for_each(|_| Ok(()))
            .map_err(|_| panic!())
    }) as Box<Future<Item = (), Error = ()>>;

    let task = Arc::new(Mutex::new(spawn(task)));
    let notify = Arc::new(MyNotify);

    let mut enter = tokio_executor::enter().unwrap();

    tokio_reactor::with_default(&reactor.handle(), &mut enter, |_| {
        let id = &*task as *const Task as usize;

        task.lock().unwrap()
            .poll_future_notify(&notify, id)
            .unwrap();
    });

    drop(task);

    // Establish a connection to the acceptor
    let _s = TcpStream::connect(&addr).unwrap();

    reactor.turn(None).unwrap();
}
