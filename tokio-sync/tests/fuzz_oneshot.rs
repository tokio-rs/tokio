#[macro_use]
extern crate futures;
#[macro_use]
extern crate loom;

#[path = "../src/oneshot.rs"]
#[allow(warnings)]
mod oneshot;

use loom::thread;
use loom::futures::wait;

#[test]
fn smoke() {
    loom::fuzz(|| {
        let (tx, rx) = oneshot::channel();

        thread::spawn(move || {
            tx.send(1);
        });

        let value = wait(rx).unwrap();
        assert_eq!(1, value);
    });
}
