extern crate env_logger;
extern crate futures;
extern crate tokio_core;
extern crate tokio_process;

use std::env;
use std::process::Command;
use std::sync::{Once, ONCE_INIT};
use std::thread;
use std::sync::mpsc::channel;

use self::tokio_core::reactor::Core;
use self::futures::future;
use self::tokio_process::CommandExt;

pub fn init() {
    static INIT: Once = ONCE_INIT;

    INIT.call_once(|| {
        drop(env_logger::init());
        let (tx, rx) = channel();
        thread::spawn(move || {
            let mut lp = Core::new().unwrap();
            let mut cmd = cmd("exit");
            let mut child = cmd.spawn_async(&lp.handle()).unwrap();
            drop(child.kill());
            lp.run(child).unwrap();
            tx.send(()).unwrap();
            drop(lp.run(future::empty::<(), ()>()));
        });
        rx.recv().unwrap();
    });
}

pub fn cmd(s: &str) -> Command {
    let mut me = env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    Command::new(me)
}
