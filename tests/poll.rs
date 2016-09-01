extern crate env_logger;
extern crate futures;
extern crate mio;
extern crate tokio_core;

use futures::{Future, Poll, Async};
use futures::task;
use tokio_core::{Loop, IoToken, LoopHandle};

struct Next(usize);

impl Future for Next {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<(), ()> {
        if self.0 == 0 {
            task::park().unpark();
            self.0 += 1;
            Ok(Async::NotReady)
        } else {
            Ok(().into())
        }
    }
}

#[test]
fn poll_after_ready() {
    drop(env_logger::init());

    let mut lp = Loop::new().unwrap();
    let handle = lp.handle();
    let (tx, rx) = mio::channel::channel::<u32>();
    let (_rx, token) = lp.run(handle.add_source(rx)).unwrap();
    tx.send(2).unwrap();
    lp.run(Next(0)).unwrap();
    lp.run(ScheduleThenPoll {
        token: token,
        handle: handle,
        n: 0,
    }).unwrap();

    struct ScheduleThenPoll {
        token: IoToken,
        handle: LoopHandle,
        n: usize,
    }

    impl Future for ScheduleThenPoll {
        type Item = ();
        type Error = ();

        fn poll(&mut self) -> Poll<(), ()> {
            if self.n == 0 {
                self.handle.schedule_read(&self.token);
                self.n += 1;
                Ok(Async::NotReady)
            } else {
                assert!(self.token.take_readiness() & 1 != 0);
                Ok(().into())
            }
        }
    }
}
