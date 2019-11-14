#![allow(warnings)]
use crate::task::{Header, Schedule, ScheduleSendOnly, Task};

use std::collections::VecDeque;
use std::sync::Mutex;
use std::thread;

pub(crate) struct Mock {
    inner: Mutex<Inner>,
}

pub(crate) struct Noop;
pub(crate) static NOOP_SCHEDULE: Noop = Noop;

struct Inner {
    calls: VecDeque<Call>,
    pending_run: VecDeque<Task<Mock>>,
    pending_drop: VecDeque<Task<Mock>>,
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}

#[derive(Debug, Eq, PartialEq)]
enum Call {
    Bind(*const Header),
    Release,
    ReleaseLocal,
    Schedule,
}

pub(crate) fn mock() -> Mock {
    Mock {
        inner: Mutex::new(Inner {
            calls: VecDeque::new(),
            pending_run: VecDeque::new(),
            pending_drop: VecDeque::new(),
        }),
    }
}

impl Mock {
    pub(crate) fn bind(self, task: &Task<Mock>) -> Self {
        self.push(Call::Bind(task.header() as *const _));
        self
    }

    pub(crate) fn release(self) -> Self {
        self.push(Call::Release);
        self
    }

    pub(crate) fn release_local(self) -> Self {
        self.push(Call::ReleaseLocal);
        self
    }

    pub(crate) fn schedule(self) -> Self {
        self.push(Call::Schedule);
        self
    }

    pub(crate) fn next_pending_run(&self) -> Option<Task<Self>> {
        self.inner.lock().unwrap().pending_run.pop_front()
    }

    pub(crate) fn next_pending_drop(&self) -> Option<Task<Self>> {
        self.inner.lock().unwrap().pending_drop.pop_front()
    }

    fn push(&self, call: Call) {
        self.inner.lock().unwrap().calls.push_back(call);
    }

    fn next(&self, name: &str) -> Call {
        self.inner
            .lock()
            .unwrap()
            .calls
            .pop_front()
            .expect(&format!("received `{}`, but none expected", name))
    }
}

impl Schedule for Mock {
    fn bind(&self, task: &Task<Self>) {
        match self.next("bind") {
            Call::Bind(ptr) => {
                assert!(ptr.eq(&(task.header() as *const _)));
            }
            call => panic!("expected `Bind`, was {:?}", call),
        }
    }

    fn release(&self, task: Task<Self>) {
        match self.next("release") {
            Call::Release => {
                self.inner.lock().unwrap().pending_drop.push_back(task);
            }
            call => panic!("expected `Release`, was {:?}", call),
        }
    }

    fn release_local(&self, _task: &Task<Self>) {
        assert_eq!(Call::ReleaseLocal, self.next("release_local"));
    }

    fn schedule(&self, task: Task<Self>) {
        self.inner.lock().unwrap().pending_run.push_back(task);
        assert_eq!(Call::Schedule, self.next("schedule"));
    }
}

impl ScheduleSendOnly for Mock {}

impl Drop for Mock {
    fn drop(&mut self) {
        if !thread::panicking() {
            assert!(self.inner.lock().unwrap().calls.is_empty());
        }
    }
}

impl Schedule for Noop {
    fn bind(&self, _task: &Task<Self>) {}

    fn release(&self, _task: Task<Self>) {}

    fn release_local(&self, _task: &Task<Self>) {}

    fn schedule(&self, _task: Task<Self>) {}
}

impl ScheduleSendOnly for Noop {}
