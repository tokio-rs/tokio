#![feature(test)]
#![deny(warnings)]

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

        b.iter(move || {
            let listener = TcpListener::bind(&addr).unwrap();
            let addr = listener.local_addr().unwrap();

            // Spawn a single future that accepts & drops connections
            let serve_incomings = listener.incoming()
                .map_err(|e| panic!("server err: {:?}", e))
                .for_each(|_| Ok(()));

            let connects = stream::iter_result((0..NUM).map(|_| {
                Ok(TcpStream::connect(&addr)
                    .and_then(|sock| {
                        sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                        read_to_end(sock, vec![])
                    }))
            }));

            let connects_concurrent = connects.buffer_unordered(CONCURRENT)
                .map_err(|e| panic!("client err: {:?}", e))
                .for_each(|_| Ok(()));

            serve_incomings.select(connects_concurrent)
                .map(|_| ()).map_err(|_| ())
                .wait().unwrap();
        });
    }

    fn n_workers(n: usize, b: &mut Bencher) {
        let (shutdown_tx, shutdown_rx) = sync::oneshot::channel();
        let (addr_tx, addr_rx) = sync::oneshot::channel();

        // Spawn reactor thread
        let server_thread = thread::spawn(move || {
            // Bind the TCP listener
            let listener = TcpListener::bind(
                &"127.0.0.1:0".parse().unwrap()).unwrap();

            // Get the address being listened on.
            let addr = listener.local_addr().unwrap();

            // Send the remote & address back to the main thread
            addr_tx.send(addr).unwrap();

            // Spawn a single future that accepts & drops connections
            let serve_incomings = listener.incoming()
                .map_err(|e| panic!("server err: {:?}", e))
                .for_each(|_| Ok(()));

            // Run server
            serve_incomings.select(shutdown_rx)
                .map(|_| ()).map_err(|_| ())
                .wait().unwrap();
        });

        // Get the bind addr of the server
        let addr = addr_rx.wait().unwrap();

        b.iter(move || {
            use std::sync::{Barrier, Arc};

            // Create a barrier to coordinate threads
            let barrier = Arc::new(Barrier::new(n + 1));

            // Spawn worker threads
            let threads: Vec<_> = (0..n).map(|_| {
                let barrier = barrier.clone();
                let addr = addr.clone();

                thread::spawn(move || {
                    let connects = stream::iter_result((0..(NUM / n)).map(|_| {
                        Ok(TcpStream::connect(&addr)
                            .map_err(|e| panic!("connect err: {:?}", e))
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

        // Shutdown the server
        shutdown_tx.send(()).unwrap();
        server_thread.join().unwrap();
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

        b.iter(move || {
            let listener = TcpListener::bind(&addr).unwrap();
            let addr = listener.local_addr().unwrap();

            // Spawn a single future that accepts 1 connection, Drain it and drops
            let server = listener.incoming()
                .into_future() // take the first connection
                .map_err(|(e, _other_incomings)| e)
                .map(|(connection, _other_incomings)| connection.unwrap())
                .and_then(|sock| {
                    sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                    let drain = Drain {
                        sock: sock,
                        chunk: read_size,
                    };
                    drain.map(|_| ()).map_err(|e| panic!("server error: {:?}", e))
                })
                .map_err(|e| panic!("server err: {:?}", e));

            let client = TcpStream::connect(&addr)
                .and_then(move |sock| {
                    Transfer {
                        sock: sock,
                        rem: MB,
                        chunk: write_size,
                    }
                })
                .map_err(|e| panic!("client err: {:?}", e));

            server.join(client).wait().unwrap();
        });
    }

    mod small_chunks {
        use ::prelude::*;

        #[bench]
        fn one_thread(b: &mut Bencher) {
            super::one_thread(b, 32, 32);
        }
    }

    mod big_chunks {
        use ::prelude::*;

        #[bench]
        fn one_thread(b: &mut Bencher) {
            super::one_thread(b, 1_024, 1_024);
        }
    }
}
