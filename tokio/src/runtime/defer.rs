use std::task::Waker;

pub(crate) struct Defer {
    deferred: Vec<Waker>,
}

impl Defer {
    pub(crate) fn new() -> Defer {
        Defer {
            deferred: Default::default(),
        }
    }

    pub(crate) fn defer(&mut self, waker: &Waker) {
        // If the same task adds itself a bunch of times, then only add it once.
        if let Some(last) = self.deferred.last() {
            if last.will_wake(waker) {
                return;
            }
        }
        self.deferred.push(waker.clone());
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.deferred.is_empty()
    }

    pub(crate) fn wake(&mut self) {
        for waker in self.deferred.drain(..) {
            waker.wake();
        }
    }

    #[cfg(tokio_taskdump)]
    pub(crate) fn take_deferred(&mut self) -> Vec<Waker> {
        std::mem::take(&mut self.deferred)
    }
}
