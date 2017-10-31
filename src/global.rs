use std::io;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use reactor::{Reactor, Handle};

struct HelperThread {
    thread: Option<thread::JoinHandle<()>>,
    done: Arc<AtomicBool>,
    reactor: Handle,
}

statik!(static DEFAULT: Option<HelperThread> = HelperThread::new().ok());

pub fn reactor() -> Option<Handle> {
    DEFAULT.with(|h| h.as_ref().map(|c| c.reactor.clone())).and_then(|x| x)
}

#[allow(dead_code)]
pub fn shutdown() {
    DEFAULT.drop();
}

impl HelperThread {
    fn new() -> io::Result<HelperThread> {
        let reactor = Reactor::new()?;
        let done = Arc::new(AtomicBool::new(false));
        let done2 = done.clone();
        let reactor_handle = reactor.handle().clone();
        let thread = thread::spawn(move || run(reactor, done2));

        Ok(HelperThread {
            thread: Some(thread),
            done: done,
            reactor: reactor_handle,
        })
    }
}

impl Drop for HelperThread {
    fn drop(&mut self) {
        self.done.store(true, Ordering::SeqCst);
        self.reactor.wakeup();
        drop(self.thread.take().unwrap().join());
    }
}

fn run(mut reactor: Reactor, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::SeqCst) {
        reactor.turn(None);
    }
}
