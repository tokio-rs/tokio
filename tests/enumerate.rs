extern crate futures;
extern crate tokio;
extern crate tokio_executor;
extern crate tokio_timer;

use futures::sync::mpsc;
use tokio::util::StreamExt;

#[test]
fn enumerate() {
    use futures::*;

    let (mut tx, rx) = mpsc::channel(1);

    std::thread::spawn(|| {
        for i in 0..5 {
            tx = tx.send(i * 2).wait().unwrap();
        }
    });

    let result = rx.enumerate().collect();
    assert_eq!(
        result.wait(),
        Ok(vec![(0, 0), (1, 2), (2, 4), (3, 6), (4, 8)])
    );
}
