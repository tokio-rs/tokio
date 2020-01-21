use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::time::{timeout, Duration};

use futures::executor::block_on;

use std::net::TcpListener;

#[test]
#[should_panic(expected = "no timer running")]
fn panics_when_no_timer() {
    block_on(timeout_value());
}

#[test]
#[should_panic(expected = "no reactor running")]
fn panics_when_no_reactor() {
    let srv = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = srv.local_addr().unwrap();
    block_on(TcpStream::connect(&addr)).unwrap();
}

async fn timeout_value() {
    let (_tx, rx) = oneshot::channel::<()>();
    let dur = Duration::from_millis(20);
    let _ = timeout(dur, rx).await;
}
