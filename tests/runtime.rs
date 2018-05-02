extern crate tokio;
extern crate env_logger;

use tokio::io;
use tokio::net::{TcpStream, TcpListener};
use tokio::prelude::*;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

fn create_client_server_future() -> Box<Future<Item=(), Error=()> + Send> {
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

    let future = server.join(client)
        .map(|_| ());
    Box::new(future)
}

#[test]
fn runtime_tokio_run() {
    let _ = env_logger::init();

    tokio::run(create_client_server_future());
}

#[test]
fn runtime_single_threaded() {
    let _ = env_logger::init();

    let mut runtime = tokio::runtime::current_thread::Runtime::new()
        .unwrap();
    runtime.block_on(create_client_server_future()).unwrap();
    runtime.run().unwrap();
}

#[test]
fn runtime_multi_threaded() {
    let _ = env_logger::init();

    let mut runtime = tokio::runtime::Builder::new()
        .build()
        .unwrap();
    runtime.spawn(create_client_server_future());
    runtime.shutdown_on_idle().wait().unwrap();
}
