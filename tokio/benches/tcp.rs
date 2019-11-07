#![warn(rust_2018_idioms)]

mod prelude {
    pub use criterion::{black_box, criterion_group, criterion_main, Bencher, Criterion};

    pub use futures::{task::Poll, *};
    pub use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    pub use std::io;
    pub use std::net::SocketAddr;
    pub use std::pin::Pin;
    pub use std::thread;
    pub use std::time::Duration;

    pub fn block_on<F>(f: F) -> F::Output
    where
        F: Future,
    {
        tokio::runtime::Runtime::new().unwrap().block_on(f)
    }
}

mod connect_churn {
    use crate::prelude::*;

    const NUM: usize = 300;
    const CONCURRENT: usize = 8;

    pub fn one_thread(b: &mut Bencher<'_>) {
        let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();

        b.iter(move || {
            let result: io::Result<_> = block_on(async {
                let listener = TcpListener::bind(&addr).await.unwrap();
                let addr = listener.local_addr().unwrap();

                // Spawn a single future that accepts & drops connections
                let serve_incomings = listener
                    .incoming()
                    .map_err(|e| panic!("server err: {:?}", e))
                    .try_for_each(|_| async { Ok(()) });

                let connects = stream::iter((0..NUM).map(|_| {
                    Ok(TcpStream::connect(&addr).and_then(|mut sock| {
                        async move {
                            sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                            sock.read_to_end(&mut vec![]).await
                        }
                    }))
                }));

                let connects_concurrent = connects
                    .try_buffer_unordered(CONCURRENT)
                    .map_err(|e| panic!("client err: {:?}", e))
                    .try_for_each(|_| async { Ok(()) });

                future::try_select(Box::pin(serve_incomings), Box::pin(connects_concurrent))
                    .map_ok(|_| ())
                    .map_err(|_| ())
                    .await
                    .unwrap();
                Ok(())
            });
            result.unwrap();
        });
    }

    fn n_workers(n: usize, b: &mut Bencher<'_>) {
        let (shutdown_tx, shutdown_rx) = channel::oneshot::channel();
        let (addr_tx, addr_rx) = channel::oneshot::channel();

        // Spawn reactor thread
        let server_thread = thread::spawn(move || {
            block_on(async {
                // Bind the TCP listener
                let listener = TcpListener::bind(&"127.0.0.1:0".parse::<SocketAddr>().unwrap())
                    .await
                    .unwrap();

                // Get the address being listened on.
                let addr = listener.local_addr().unwrap();

                // Send the remote & address back to the main thread
                addr_tx.send(addr).unwrap();

                // Spawn a single future that accepts & drops connections
                let serve_incomings = listener
                    .incoming()
                    .map_err(|e| panic!("server err: {:?}", e))
                    .try_for_each(|_| async { Ok(()) });

                // Run server
                future::try_select(Box::pin(serve_incomings), Box::pin(shutdown_rx))
                    .map_ok(|_| ())
                    .map_err(|_| ())
                    .await
            })
            .unwrap();
        });

        // Get the bind addr of the server
        let addr = block_on(addr_rx).unwrap();

        b.iter(move || {
            use std::sync::{Arc, Barrier};

            // Create a barrier to coordinate threads
            let barrier = Arc::new(Barrier::new(n + 1));

            // Spawn worker threads
            let threads: Vec<_> = (0..n)
                .map(|_| {
                    let barrier = barrier.clone();
                    let addr = addr.clone();

                    thread::spawn(move || {
                        let connects = stream::iter((0..(NUM / n)).map(|_| {
                            Ok(TcpStream::connect(&addr)
                                .map_err(|e| panic!("connect err: {:?}", e))
                                .and_then(|mut sock| {
                                    async move {
                                        sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                                        sock.read_to_end(&mut vec![]).await
                                    }
                                }))
                        }));

                        barrier.wait();

                        block_on(
                            connects
                                .try_buffer_unordered(CONCURRENT)
                                .map_err(|e| panic!("client err: {:?}", e))
                                .try_for_each(|_| async { Ok(()) }),
                        )
                        .unwrap();
                    })
                })
                .collect();

            barrier.wait();

            for th in threads {
                th.join().unwrap();
            }
        });

        // Shutdown the server
        shutdown_tx.send(()).unwrap();
        server_thread.join().unwrap();
    }

    pub fn two_threads(b: &mut Bencher<'_>) {
        n_workers(1, b);
    }

    pub fn multi_threads(b: &mut Bencher<'_>) {
        n_workers(4, b);
    }
}

mod transfer {
    use crate::prelude::*;
    use std::{cmp, mem};

    const MB: usize = 3 * 1024 * 1024;

    struct Drain {
        sock: TcpStream,
        chunk: usize,
    }

    impl Future for Drain {
        type Output = io::Result<()>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
            let mut buf: [u8; 1024] = unsafe { mem::MaybeUninit::uninit().assume_init() };

            let self_ = &mut *self;
            loop {
                match ready!(Pin::new(&mut self_.sock).poll_read(cx, &mut buf[..self_.chunk]))? {
                    0 => return Poll::Ready(Ok(())),
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
        type Output = io::Result<()>;

        fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<io::Result<()>> {
            while self.rem > 0 {
                let len = cmp::min(self.rem, self.chunk);
                let buf = &DATA[..len];

                let n = ready!(Pin::new(&mut self.sock).poll_write(cx, &buf))?;
                self.rem -= n;
            }

            Poll::Ready(Ok(()))
        }
    }

    static DATA: [u8; 1024] = [0; 1024];

    fn one_thread(b: &mut Bencher<'_>, read_size: usize, write_size: usize) {
        let addr = "127.0.0.1:0".parse::<SocketAddr>().unwrap();

        b.iter(move || {
            let result: io::Result<_> = block_on(async move {
                let listener = TcpListener::bind(&addr).await?;
                let addr = listener.local_addr().unwrap();

                // Spawn a single future that accepts 1 connection, Drain it and drops
                let server = async move {
                    // take the first connection
                    let sock = listener.incoming().into_future().await.0.unwrap()?;
                    sock.set_linger(Some(Duration::from_secs(0))).unwrap();
                    let drain = Drain {
                        sock,
                        chunk: read_size,
                    };
                    drain.map_ok(|_| ()).await
                };

                let client = TcpStream::connect(&addr).and_then(move |sock| Transfer {
                    sock,
                    rem: MB,
                    chunk: write_size,
                });

                future::try_join(server, client).await
            });
            result.unwrap()
        });
    }

    pub mod small_chunks {
        use crate::prelude::*;

        pub fn one_thread(b: &mut Bencher<'_>) {
            super::one_thread(b, 32, 32);
        }
    }

    pub mod big_chunks {
        use crate::prelude::*;

        pub fn one_thread(b: &mut Bencher<'_>) {
            super::one_thread(b, 1_024, 1_024);
        }
    }
}

use prelude::*;

fn bench_tcp(c: &mut Criterion) {
    c.bench_function("connect_churn/one_thread", connect_churn::one_thread);
    c.bench_function("connect_churn/two_threads", connect_churn::two_threads);
    c.bench_function("connect_churn/multi_threads", connect_churn::multi_threads);

    c.bench_function("transfer/small_chunks", transfer::small_chunks::one_thread);
    c.bench_function("transfer/big_chunks", transfer::big_chunks::one_thread);
}

criterion_group!(tcp, bench_tcp);
criterion_main!(tcp);
