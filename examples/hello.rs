extern crate futures;
extern crate tokio_core;

use futures::stream::Stream;
use tokio_core::reactor::Core;
use tokio_core::net::TcpListener;

fn main() {
    let mut core = Core::new().unwrap();
    let address = "127.0.0.1:8080".parse().unwrap();
    let listener = TcpListener::bind(&address, &core.handle()).unwrap();

    let addr = listener.local_addr().unwrap();
    println!("Listening for connections on {}", addr);

    let clients = listener.incoming();
    let welcomes = clients.and_then(|(socket, _peer_addr)| {
        tokio_core::io::write_all(socket, b"Hello!\n")
    });
    let server = welcomes.for_each(|(_socket, _welcome)| {
        Ok(())
    });

    core.run(server).unwrap();
}
