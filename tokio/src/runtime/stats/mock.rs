//! This file contains mocks of the types in src/runtime/stats/stats.rs

pub(crate) struct RuntimeStats {}

impl RuntimeStats {
    pub(crate) fn new(_worker_threads: usize) -> Self {
        Self {}
    }
}

pub(crate) struct WorkerStatsBatcher {}

impl WorkerStatsBatcher {
    pub(crate) fn new(_my_index: usize) -> Self {
        Self {}
    }

    pub(crate) fn submit(&mut self, _to: &RuntimeStats) {}

    pub(crate) fn about_to_park(&mut self) {}
    pub(crate) fn returned_from_park(&mut self) {}

    #[cfg(feature = "rt-multi-thread")]
    pub(crate) fn incr_steal_count(&mut self, _by: u16) {}

    pub(crate) fn incr_poll_count(&mut self) {}
}
