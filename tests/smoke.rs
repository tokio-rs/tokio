extern crate futures;
extern crate tokio_core;
extern crate tokio_process;

use std::env;
use std::sync::mpsc::channel;
use std::sync::{Once, ONCE_INIT};
use std::thread;
use std::process::Command;

use tokio_core::reactor::Core;
use tokio_process::CommandExt;

static INIT: Once = ONCE_INIT;

fn init() {
    INIT.call_once(|| {
        let (tx, rx) = channel();
        thread::spawn(move || {
            let mut lp = Core::new().unwrap();
            let mut cmd = exit();
            let mut child = cmd.spawn_async(&lp.handle()).unwrap();
            drop(child.kill());
            lp.run(child).unwrap();
            tx.send(()).unwrap();
            drop(lp.run(futures::empty::<(), ()>()));
        });
        rx.recv().unwrap();
    });
}

fn exit() -> Command {
    let mut me = env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push("exit");
    Command::new(me)
}

#[test]
fn simple() {
    init();

    let mut lp = Core::new().unwrap();
    let mut cmd = exit();
    cmd.arg("2");
    let mut child = cmd.spawn_async(&lp.handle()).unwrap();
    let id = child.id();
    assert!(id > 0);
    let status = lp.run(&mut child).unwrap();
    assert_eq!(status.code(), Some(2));
    assert_eq!(child.id(), id);
    drop(child.kill());
}
