use std::io;
use std::thread;

use reactor::{Reactor, Handle};

pub struct HelperThread {
    thread: Option<thread::JoinHandle<()>>,
    reactor: Handle,
}

impl HelperThread {
    pub fn new() -> io::Result<HelperThread> {
        let reactor = Reactor::new()?;
        let reactor_handle = reactor.handle().clone();
        let thread = thread::Builder::new().spawn(move || run(reactor))?;

        Ok(HelperThread {
            thread: Some(thread),
            reactor: reactor_handle,
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
        // TODO: kill the reactor thread and wait for it to exit, needs
        //       `Handle::wakeup` to be implemented in a future PR
    }
}

fn run(mut reactor: Reactor) {
    loop {
        reactor.turn(None);
    }
}
