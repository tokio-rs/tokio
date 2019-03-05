mod common;

use futures::Future;
use std::io;
use tokio_current_thread::CurrentThread;

pub fn run<F>(f: F)
where
    F: Future<Item = (), Error = io::Error> + Send + 'static,
{
    CurrentThread::new().block_on(f).unwrap();
}
