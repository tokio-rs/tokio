extern crate futures;
extern crate tokio_core;
extern crate tokio_process;

use std::env;
use std::sync::mpsc::channel;
use std::sync::{Once, ONCE_INIT};
use std::thread;

use tokio_core::{Loop, LoopHandle};
use tokio_process::Command;

static INIT: Once = ONCE_INIT;

fn init() {
    INIT.call_once(|| {
        let (tx, rx) = channel();
        thread::spawn(move || {
            let mut lp = Loop::new().unwrap();
            let cmd = exit(&lp.handle());
            let mut child = lp.run(cmd.spawn()).unwrap();
            drop(child.kill());
            lp.run(child).unwrap();
            tx.send(()).unwrap();
            drop(lp.run(futures::empty::<(), ()>()));
        });
        rx.recv().unwrap();
    });
}

fn exit(handle: &LoopHandle) -> Command {
    let mut me = env::current_exe().unwrap();
    me.pop();
    me.push("exit");
    Command::new(me, handle)
}

#[test]
fn simple() {
    init();

    let mut lp = Loop::new().unwrap();
    let mut cmd = exit(&lp.handle());
    cmd.arg("2");
    let mut child = lp.run(cmd.spawn()).unwrap();
    let id = child.id();
    assert!(id > 0);
    let status = lp.run(&mut child).unwrap();
    assert_eq!(status.code(), Some(2));
    assert_eq!(child.id(), id);
    drop(child.kill());
}
