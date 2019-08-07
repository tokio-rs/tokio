#![deny(warnings, rust_2018_idioms)]

use futures_util::future;
use futures_util::future::FutureExt;
use std::env;
use std::future::Future;
use std::process::Command;
use std::time::Duration;
use tokio::timer::Timeout;

#[allow(dead_code)]
pub fn cmd(s: &str) -> Command {
    let mut me = env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    Command::new(me)
}

pub fn with_timeout<F: Future>(future: F) -> impl Future<Output = F::Output> {
    Timeout::new(future, Duration::from_secs(3)).then(|r| {
        if r.is_err() {
            panic!("timed out {:?}", r.err());
        }
        future::ready(r.unwrap())
    })
}
