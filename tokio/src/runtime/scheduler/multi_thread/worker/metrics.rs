use super::Shared;

impl Shared {
    pub(crate) fn injection_queue_depth(&self, group: usize) -> usize {
        self.injects[group].len()
    }
}

cfg_unstable_metrics! {
    impl Shared {
        pub(crate) fn worker_local_queue_depth(&self, worker: usize) -> usize {
            self.remotes[worker].steal.len()
        }
    }
}
