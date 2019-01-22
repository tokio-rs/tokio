extern crate futures;
extern crate loom;

#[path = "../src/oneshot.rs"]
#[allow(warnings)]
mod oneshot;

use loom::thread;
use loom::futures::block_on;

#[test]
fn smoke() {
    loom::fuzz(|| {
        let (tx, rx) = oneshot::channel();

        thread::spawn(move || {
            tx.send(1).unwrap();
        });

        let value = block_on(rx).unwrap();
        assert_eq!(1, value);
    });
}
