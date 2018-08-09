extern crate futures;
extern crate mio;
extern crate tokio;
extern crate tokio_reactor;

use futures::Async;
use mio::Ready;
use tokio::prelude::*;
use tokio_reactor::Registration;

const NUM_FUTURES: usize = 1000;
const NUM_STEPS: usize = 1000;

fn main() {
    tokio::run(future::lazy(|| {
        for _ in 0..NUM_FUTURES {
            let (r, s) = mio::Registration::new2();
            let registration = Registration::new();
            registration.register(&r).unwrap();

            let mut r = Some(r);
            let mut step = 0;

            tokio::spawn(future::poll_fn(move || {
                loop {
                    let is_ready = registration.poll_read_ready().unwrap().is_ready();

                    if is_ready {
                        step += 1;

                        if step == NUM_STEPS {
                            r.take().unwrap();
                            return Ok(Async::Ready(()));
                        }
                    } else {
                        s.set_readiness(Ready::readable()).unwrap();
                        return Ok(Async::NotReady);
                    }
                }
            }));
        }

        Ok(())
    }));
}
