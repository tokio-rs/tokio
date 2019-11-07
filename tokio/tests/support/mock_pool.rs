use tokio::sync::oneshot;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

thread_local! {
    static QUEUE: RefCell<VecDeque<Box<dyn FnOnce() + Send>>> = RefCell::new(VecDeque::new())
}

#[derive(Debug)]
pub(crate) struct Blocking<T> {
    rx: oneshot::Receiver<T>,
}

pub(crate) fn run<F, R>(f: F) -> Blocking<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let task = Box::new(move || {
        let _ = tx.send(f());
    });

    QUEUE.with(|cell| cell.borrow_mut().push_back(task));

    Blocking { rx }
}

impl<T> Future for Blocking<T> {
    type Output = Result<T, io::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use std::task::Poll::*;

        match Pin::new(&mut self.rx).poll(cx) {
            Ready(Ok(v)) => Ready(Ok(v)),
            Ready(Err(e)) => panic!("error = {:?}", e),
            Pending => Pending,
        }
    }
}

pub(crate) async fn asyncify<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T> + Send + 'static,
    T: Send + 'static,
{
    run(f).await?
}

pub(crate) fn len() -> usize {
    QUEUE.with(|cell| cell.borrow().len())
}

pub(crate) fn run_one() {
    let task = QUEUE
        .with(|cell| cell.borrow_mut().pop_front())
        .expect("expected task to run, but none ready");

    task();
}
