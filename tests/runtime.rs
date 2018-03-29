extern crate futures;
extern crate tokio;
extern crate tokio_io;
extern crate env_logger;

use futures::prelude::*;
use tokio::net::{TcpStream, TcpListener};
use tokio_io::io;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn basic_runtime_usage() {
    let _ = env_logger::init();

    // TODO: Don't require the lazy wrapper
    tokio::run(::futures::future::lazy(|| {
        let server = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap()));
        let addr = t!(server.local_addr());
        let client = TcpStream::connect(&addr);

        let server = server.incoming().take(1)
            .map_err(|e| panic!("accept err = {:?}", e))
            .for_each(|socket| {
                tokio::spawn({
                    io::write_all(socket, b"hello")
                        .map(|_| ())
                        .map_err(|e| panic!("write err = {:?}", e))
                })
            })
            .map(|_| ());

        let client = client
            .map_err(|e| panic!("connect err = {:?}", e))
            .and_then(|client| {
                // Read all
                io::read_to_end(client, vec![])
                    .map(|_| ())
                    .map_err(|e| panic!("read err = {:?}", e))
            });

        tokio::spawn({
            server.join(client)
                .map(|_| ())
        })
    }));
}
