use super::Shared;

impl Shared {
    pub(crate) fn injection_queue_depth(&self) -> usize {
        self.inject.len()
    }

    pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
        self.remotes[worker].steal.len()
    }
}
