use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

#[derive(Default)]
pub(crate) struct TimerDriverMetrics {
    pub(super) entry_count: AtomicU64,
}

impl TimerDriverMetrics {
    pub(crate) fn incr_entry_count(&self) {
        let prev = self.entry_count.load(Relaxed);
        let new = prev.wrapping_add(1);
        self.entry_count.store(new, Relaxed);
    }

    pub(crate) fn dec_entry_count(&self) {
        let prev = self.entry_count.load(Relaxed);
        let new = prev.wrapping_sub(1);
        self.entry_count.store(new, Relaxed);
    }
}
