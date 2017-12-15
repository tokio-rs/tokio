#![feature(test)]

extern crate futures;
extern crate tokio;

#[macro_use]
extern crate tokio_io;

pub extern crate test;

mod prelude {
    pub use futures::*;
    pub use tokio::reactor::Reactor;
    pub use tokio::net::{TcpListener, TcpStream};
    pub use tokio_io::io::read_to_end;

    pub use test::{self, Bencher};
    pub use std::thread;
    pub use std::time::Duration;
    pub use std::io::{self, Read, Write};
}

mod connect_churn {
    use ::prelude::*;

    const NUM: usize = 300;
    const CONCURRENT: usize = 8;

    #[bench]
    fn one_thread(b: &mut Bencher) {
        let addr = "127.0.0.1:0".parse().unwrap();
        let mut core = Reactor::new().unwrap();
        let handle = core.handle();
        let listener = TcpListener::bind(&addr, &handle).unwrap();
        let addr = listener.local_addr().unwrap();

        // Spawn a single task that accepts & drops connections
        handle.spawn(
            listener.incoming()
                .map_err(|e| panic!("server err: {:?}", e))
                .for_each(|_| Ok(())));

        b.iter(move || {
            let connects = stream::iter((0..NUM).map(|_| {
                Ok(TcpStream::connect(&addr, &handle)
                    .and_then(|sock| {
                        sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                        read_to_end(sock, vec![])
                    }))
            }));

            core.run(
                connects.buffer_unordered(CONCURRENT)
                    .map_err(|e| panic!("client err: {:?}", e))
                    .for_each(|_| Ok(()))).unwrap();
        });
    }

    fn n_workers(n: usize, b: &mut Bencher) {
        let (shutdown_tx, shutdown_rx) = sync::oneshot::channel();
        let (remote_tx, remote_rx) = ::std::sync::mpsc::channel();

        // Spawn reactor thread
        thread::spawn(move || {
            // Create the core
            let mut core = Reactor::new().unwrap();

            // Reactor handles
            let handle = core.handle();
            let remote = handle.remote().clone();

            // Bind the TCP listener
            let listener = TcpListener::bind(
                &"127.0.0.1:0".parse().unwrap(), &handle).unwrap();

            // Get the address being listened on.
            let addr = listener.local_addr().unwrap();

            // Send the remote & address back to the main thread
            remote_tx.send((remote, addr)).unwrap();

            // Spawn a single task that accepts & drops connections
            handle.spawn(
                listener.incoming()
                    .map_err(|e| panic!("server err: {:?}", e))
                    .for_each(|_| Ok(())));

            // Run the reactor
            core.run(shutdown_rx).unwrap();
        });

        // Get the remote info
        let (remote, addr) = remote_rx.recv().unwrap();

        b.iter(move || {
            use std::sync::{Barrier, Arc};

            // Create a barrier to coordinate threads
            let barrier = Arc::new(Barrier::new(n + 1));

            // Spawn worker threads
            let threads: Vec<_> = (0..n).map(|_| {
                let barrier = barrier.clone();
                let remote = remote.clone();
                let addr = addr.clone();

                thread::spawn(move || {
                    let connects = stream::iter((0..(NUM / n)).map(|_| {
                        // TODO: Once `Handle` is `Send / Sync`, update this

                        let (socket_tx, socket_rx) = sync::oneshot::channel();

                        remote.spawn(move |handle| {
                            TcpStream::connect(&addr, &handle)
                                .map_err(|e| panic!("connect err: {:?}", e))
                                .then(|res| socket_tx.send(res))
                                .map_err(|_| ())
                        });

                        Ok(socket_rx
                            .then(|res| res.unwrap())
                            .and_then(|sock| {
                                sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                                read_to_end(sock, vec![])
                            }))
                    }));

                    barrier.wait();

                    connects.buffer_unordered(CONCURRENT)
                        .map_err(|e| panic!("client err: {:?}", e))
                        .for_each(|_| Ok(())).wait().unwrap();
                })
            }).collect();

            barrier.wait();

            for th in threads {
                th.join().unwrap();
            }
        });

        // Shutdown the reactor
        shutdown_tx.send(()).unwrap();
    }

    #[bench]
    fn two_threads(b: &mut Bencher) {
        n_workers(1, b);
    }

    #[bench]
    fn multi_threads(b: &mut Bencher) {
        n_workers(4, b);
    }
}

mod transfer {
    use ::prelude::*;
    use std::{cmp, mem};

    const MB: usize = 3 * 1024 * 1024;

    struct Drain {
        sock: TcpStream,
        chunk: usize,
    }

    impl Future for Drain {
        type Item = ();
        type Error = io::Error;

        fn poll(&mut self) -> Poll<(), io::Error> {
            let mut buf: [u8; 1024] = unsafe { mem::uninitialized() };

            loop {
                match try_nb!(self.sock.read(&mut buf[..self.chunk])) {
                    0 => return Ok(Async::Ready(())),
                    _ => {}
                }
            }
        }
    }

    struct Transfer {
        sock: TcpStream,
        rem: usize,
        chunk: usize,
    }

    impl Future for Transfer {
        type Item = ();
        type Error = io::Error;

        fn poll(&mut self) -> Poll<(), io::Error> {
            while self.rem > 0 {
                let len = cmp::min(self.rem, self.chunk);
                let buf = &DATA[..len];

                let n = try_nb!(self.sock.write(&buf));
                self.rem -= n;
            }

            Ok(Async::Ready(()))
        }
    }

    static DATA: [u8; 1024] = [0; 1024];

    fn one_thread(b: &mut Bencher, read_size: usize, write_size: usize) {
        let addr = "127.0.0.1:0".parse().unwrap();
        let mut core = Reactor::new().unwrap();
        let handle = core.handle();
        let listener = TcpListener::bind(&addr, &handle).unwrap();
        let addr = listener.local_addr().unwrap();

        let h2 = handle.clone();

        // Spawn a single task that accepts & drops connections
        handle.spawn(
            listener.incoming()
                .map_err(|e| panic!("server err: {:?}", e))
                .for_each(move |(sock, _)| {
                    sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                    let drain = Drain {
                        sock: sock,
                        chunk: read_size,
                    };

                    h2.spawn(drain.map_err(|e| panic!("server error: {:?}", e)));

                    Ok(())
                }));

        b.iter(move || {
            let client = TcpStream::connect(&addr, &handle)
                .and_then(|sock| {
                    Transfer {
                        sock: sock,
                        rem: MB,
                        chunk: write_size,
                    }
                });

            core.run(
                client.map_err(|e| panic!("client err: {:?}", e))
                ).unwrap();
        });
    }

    fn cross_thread(b: &mut Bencher, read_size: usize, write_size: usize) {
        let (shutdown_tx, shutdown_rx) = sync::oneshot::channel();
        let (remote_tx, remote_rx) = ::std::sync::mpsc::channel();

        // Spawn reactor thread
        thread::spawn(move || {
            // Create the core
            let mut core = Reactor::new().unwrap();

            // Reactor handles
            let handle = core.handle();
            let remote = handle.remote().clone();

            remote_tx.send(remote).unwrap();
            core.run(shutdown_rx).unwrap();
        });

        let remote = remote_rx.recv().unwrap();

        b.iter(move || {
            let (server_tx, server_rx) = sync::oneshot::channel();
            let (client_tx, client_rx) = sync::oneshot::channel();

            remote.spawn(|handle| {
                let sock = TcpListener::bind(&"127.0.0.1:0".parse().unwrap(), &handle).unwrap();
                server_tx.send(sock).unwrap();
                Ok(())
            });

            let remote2 = remote.clone();

            server_rx.and_then(move |server| {
                let addr = server.local_addr().unwrap();

                remote2.spawn(move |handle| {
                    let fut = TcpStream::connect(&addr, &handle);
                    client_tx.send(fut).ok().unwrap();
                    Ok(())
                });

                let client = client_rx
                    .then(|res| res.unwrap())
                    .and_then(move |sock| {
                        Transfer {
                            sock: sock,
                            rem: MB,
                            chunk: write_size,
                        }
                    });

                let server = server.incoming().into_future()
                    .map_err(|(e, _)| e)
                    .and_then(move |(sock, _)| {
                        let sock = sock.unwrap().0;
                        sock.set_linger(Some(Duration::from_secs(0))).unwrap();

                        Drain {
                            sock: sock,
                            chunk: read_size,
                        }
                    });

                client
                    .join(server)
                    .then(|res| {
                        let _ = res.unwrap();
                        Ok(())
                    })
            }).wait().unwrap();
        });

        // Shutdown the reactor
        shutdown_tx.send(()).unwrap();
    }

    mod small_chunks {
        use ::prelude::*;

        #[bench]
        fn one_thread(b: &mut Bencher) {
            super::one_thread(b, 32, 32);
        }

        #[bench]
        fn cross_thread(b: &mut Bencher) {
            super::cross_thread(b, 32, 32);
        }
    }

    mod big_chunks {
        use ::prelude::*;

        #[bench]
        fn one_thread(b: &mut Bencher) {
            super::one_thread(b, 1_024, 1_024);
        }

        #[bench]
        fn cross_thread(b: &mut Bencher) {
            super::cross_thread(b, 1_024, 1_024);
        }
    }
}
