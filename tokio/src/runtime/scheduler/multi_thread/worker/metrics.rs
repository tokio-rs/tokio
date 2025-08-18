use super::Shared;

impl Shared {
    pub(crate) fn injection_queue_depth(&self) -> usize {
        self.inject.len()
    }
}

cfg_unstable_metrics! {
    impl Shared {
        pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
            self.remotes[worker].steal.len()
        }

        pub(crate) fn worker_local_queue_depth_checked(&self, worker: usize) -> Option<usize> {
            self.remotes.get(worker).map(|worker| {
                worker.steal.len()
            })
        }
    }
}
