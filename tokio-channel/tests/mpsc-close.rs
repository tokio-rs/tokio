extern crate tokio_channel;
extern crate futures;


use tokio_channel::mpsc::*;
use futures::prelude::*;
use std::thread;

#[test]
fn smoke() {
    let (mut sender, receiver) = channel(1);

    let t = thread::spawn(move ||{
        while let Ok(s) = sender.send(42).wait() {
            sender = s;
        }
    });

    receiver.take(3).for_each(|_| Ok(())).wait().unwrap();

    t.join().unwrap()
}
