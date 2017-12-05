extern crate futures;
extern crate tokio;

use std::thread;

use futures::prelude::*;
use tokio::net::{TcpStream, TcpListener};
use tokio::reactor::Handle;

macro_rules! t {
    ($e:expr) => (match $e {
        Ok(e) => e,
        Err(e) => panic!("{} failed with {:?}", stringify!($e), e),
    })
}

#[test]
fn hammer() {
    let threads = (0..10).map(|_| {
        thread::spawn(|| {
            let handle = Handle::default();
            let srv = t!(TcpListener::bind(&"127.0.0.1:0".parse().unwrap(), &handle));
            let addr = t!(srv.local_addr());
            let mine = TcpStream::connect(&addr, &handle);
            let theirs = srv.incoming().into_future()
                .map(|(s, _)| s.unwrap().0)
                .map_err(|(s, _)| s);
            let (mine, theirs) = t!(mine.join(theirs).wait());

            assert_eq!(t!(mine.local_addr()), t!(theirs.peer_addr()));
            assert_eq!(t!(theirs.local_addr()), t!(mine.peer_addr()));
        })
    }).collect::<Vec<_>>();
    for thread in threads {
        thread.join().unwrap();
    }
}
