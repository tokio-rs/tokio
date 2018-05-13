extern crate env_logger;
extern crate futures;
extern crate tokio_process;

use std::env;
use std::process::Command;

pub fn cmd(s: &str) -> Command {
    let mut me = env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    Command::new(me)
}
