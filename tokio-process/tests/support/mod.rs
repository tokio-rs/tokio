extern crate tokio;

use futures_util::future;
use futures_util::future::FutureExt;
use std::env;
use std::future::Future;
use std::process::Command;
use std::time::Duration;
use tokio::timer::Timeout;

pub use self::tokio::runtime::current_thread::Runtime as CurrentThreadRuntime;

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

pub fn run_with_timeout<F>(future: F) -> F::Output
where
    F: Future,
{
    // NB: Timeout requires a timer registration which is provided by
    // tokio's `current_thread::Runtime`, but isn't available by just using
    // tokio's default CurrentThread executor which powers `current_thread::block_on_all`.
    let mut rt = CurrentThreadRuntime::new().expect("failed to get runtime");
    rt.block_on(with_timeout(future))
}
