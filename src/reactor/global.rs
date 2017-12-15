use std::io;
use std::thread;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use reactor::{Reactor, Handle};

pub struct HelperThread {
    thread: Option<thread::JoinHandle<()>>,
    reactor: Handle,
    done: Arc<AtomicBool>,
}

impl HelperThread {
    pub fn new() -> io::Result<HelperThread> {
        let reactor = Reactor::new()?;
        let reactor_handle = reactor.handle().clone();
        let done = Arc::new(AtomicBool::new(false));
        let done2 = done.clone();
        let thread = thread::Builder::new().spawn(move || run(reactor, done))?;

        Ok(HelperThread {
            thread: Some(thread),
            reactor: reactor_handle,
            done: done2,
        })
    }

    pub fn handle(&self) -> &Handle {
        &self.reactor
    }

    pub fn forget(mut self) {
        drop(self.thread.take());
    }
}

impl Drop for HelperThread {
    fn drop(&mut self) {
        let thread = match self.thread.take() {
            Some(thread) => thread,
            None => return
        };
        self.done.store(true, Ordering::SeqCst);
        self.reactor.wakeup();
        thread.join().unwrap();
    }
}

fn run(mut reactor: Reactor, done: Arc<AtomicBool>) {
    while !done.load(Ordering::SeqCst) {
        reactor.turn(None);
    }
}
