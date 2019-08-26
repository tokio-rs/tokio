use tokio_sync::oneshot;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::io;

thread_local! {
    static QUEUE: RefCell<VecDeque<Box<dyn Task>>> = RefCell::new(VecDeque::new())
}

pub(crate) fn run<F, R>(f: F) -> oneshot::Receiver<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let task = Box::new(move || {
        let _ = tx.send(f());
    });

    QUEUE.with(|cell| cell.borrow_mut().push_back(task));

    rx
}

pub(crate) async fn asyncify<F, T>(f: F) -> io::Result<T>
where
    F: FnOnce() -> io::Result<T> + Send + 'static,
    T: Send + 'static,
{
    run(f).await.unwrap()
}

pub(crate) fn len() -> usize {
    QUEUE.with(|cell| cell.borrow().len())
}

pub(crate) fn run_one() {
    let task = QUEUE
        .with(|cell| cell.borrow_mut().pop_front())
        .expect("expected task to run, but none ready");

    task.run();
}

trait Task {
    fn run(self: Box<Self>);
}

impl<F: FnOnce()> Task for F {
    fn run(self: Box<Self>) {
        self();
    }
}
