#![feature(test)]

extern crate futures;
extern crate mio;
extern crate num_cpus;
extern crate test;
extern crate tokio;
extern crate tokio_io_pool;
extern crate tokio_reactor;

const NUM_YIELD: usize = 500;
const TASKS_PER_CPU: usize = 100;

mod threadpool {
    use super::*;
    use std::sync::mpsc;

    use futures::{future, Async};
    use test::Bencher;
    use tokio::runtime::Runtime;
    use tokio_reactor::Registration;

    #[bench]
    fn notify_many(b: &mut Bencher) {
        let mut rt = Runtime::new().unwrap();
        let tasks = TASKS_PER_CPU * num_cpus::get();

        b.iter(|| {
            let (tx, rx) = mpsc::channel();

            rt.block_on::<_, (), ()>(future::lazy(move || {
                for _ in 0..tasks {
                    let tx = tx.clone();

                    tokio::spawn(future::lazy(move || {
                        let (r, s) = mio::Registration::new2();
                        let registration = Registration::new();
                        registration.register(&r).unwrap();

                        let mut rem = NUM_YIELD;
                        let mut r = Some(r);
                        let tx = tx.clone();

                        tokio::spawn(future::poll_fn(move || loop {
                            let is_ready = registration.poll_read_ready().unwrap().is_ready();

                            if is_ready {
                                rem -= 1;

                                if rem == 0 {
                                    r.take().unwrap();
                                    tx.send(()).unwrap();
                                    return Ok(Async::Ready(()));
                                }
                            } else {
                                s.set_readiness(mio::Ready::readable()).unwrap();
                                return Ok(Async::NotReady);
                            }
                        }));

                        Ok(())
                    }));
                }

                Ok(())
            }))
            .unwrap();

            for _ in 0..tasks {
                rx.recv().unwrap();
            }
        })
    }
}

mod io_pool {
    use super::*;
    use std::sync::mpsc;

    use futures::{future, Async};
    use test::Bencher;
    use tokio_io_pool::Runtime;
    use tokio_reactor::Registration;

    #[bench]
    fn notify_many(b: &mut Bencher) {
        let mut rt = Runtime::new();
        let tasks = TASKS_PER_CPU * num_cpus::get();

        b.iter(|| {
            let (tx, rx) = mpsc::channel();

            rt.block_on::<_, (), ()>(future::lazy(move || {
                for _ in 0..tasks {
                    let tx = tx.clone();

                    tokio::spawn(future::lazy(move || {
                        let (r, s) = mio::Registration::new2();
                        let registration = Registration::new();
                        registration.register(&r).unwrap();

                        let mut rem = NUM_YIELD;
                        let mut r = Some(r);
                        let tx = tx.clone();

                        tokio::spawn(future::poll_fn(move || loop {
                            let is_ready = registration.poll_read_ready().unwrap().is_ready();

                            if is_ready {
                                rem -= 1;

                                if rem == 0 {
                                    r.take().unwrap();
                                    tx.send(()).unwrap();
                                    return Ok(Async::Ready(()));
                                }
                            } else {
                                s.set_readiness(mio::Ready::readable()).unwrap();
                                return Ok(Async::NotReady);
                            }
                        }));

                        Ok(())
                    }));
                }

                Ok(())
            }))
            .unwrap();

            for _ in 0..tasks {
                rx.recv().unwrap();
            }
        })
    }
}
