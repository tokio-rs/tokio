extern crate futures;
extern crate tokio;

use self::futures::Future;
use self::tokio::timer::Timeout;
use std::env;
use std::process::Command;
use std::time::Duration;

pub use self::tokio::runtime::current_thread::Runtime as CurrentThreadRuntime;

pub fn cmd(s: &str) -> Command {
    let mut me = env::current_exe().unwrap();
    me.pop();
    if me.ends_with("deps") {
        me.pop();
    }
    me.push(s);
    Command::new(me)
}

pub fn with_timeout<F: Future>(future: F) -> impl Future<Item = F::Item, Error = F::Error> {
    Timeout::new(future, Duration::from_secs(3)).map_err(|e| {
        if e.is_timer() {
            panic!("failed to register timer");
        } else if e.is_elapsed() {
            panic!("timed out")
        } else {
            e.into_inner().expect("missing inner error")
        }
    })
}

pub fn run_with_timeout<F>(future: F) -> Result<F::Item, F::Error>
where
    F: Future,
{
    // NB: Timeout requires a timer registration which is provided by
    // tokio's `current_thread::Runtime`, but isn't available by just using
    // tokio's default CurrentThread executor which powers `current_thread::block_on_all`.
    let mut rt = CurrentThreadRuntime::new().expect("failed to get runtime");
    rt.block_on(with_timeout(future))
}
