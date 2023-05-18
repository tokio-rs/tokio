#[cfg(tokio_internal_mt_counters)]
mod imp {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::Relaxed;

    static NUM_MAINTENANCE: AtomicUsize = AtomicUsize::new(0);
    static NUM_NOTIFY_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_UNPARKS_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_NEED_SEARCHERS: AtomicUsize = AtomicUsize::new(0);
    static NUM_WAKE_DEFERS: AtomicUsize = AtomicUsize::new(0);
    static NUM_WAKE_DEFERS_MULT: AtomicUsize = AtomicUsize::new(0);

    impl Drop for super::Counters {
        fn drop(&mut self) {
            let notifies_local = NUM_NOTIFY_LOCAL.load(Relaxed);
            let unparks_local = NUM_UNPARKS_LOCAL.load(Relaxed);
            let maintenance = NUM_MAINTENANCE.load(Relaxed);
            let need_searchers = NUM_NEED_SEARCHERS.load(Relaxed);
            let defers = NUM_WAKE_DEFERS.load(Relaxed);
            let defers_mult = NUM_WAKE_DEFERS_MULT.load(Relaxed);

            println!("---");
            println!("notifies (local): {}", notifies_local);
            println!(" unparks (local): {}", unparks_local);
            println!("     maintenance: {}", maintenance);
            println!("  need_searchers: {}", need_searchers);
            println!("   waking defers: {}", defers);
            println!("          (mult): {}", defers_mult);
        }
    }

    pub(crate) fn inc_num_inc_notify_local() {
        NUM_NOTIFY_LOCAL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_unparks_local() {
        NUM_UNPARKS_LOCAL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_maintenance() {
        NUM_MAINTENANCE.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_need_searchers() {
        NUM_NEED_SEARCHERS.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_defers(batch: usize) {
        NUM_WAKE_DEFERS.fetch_add(1, Relaxed);

        if batch > 1 {
            NUM_WAKE_DEFERS_MULT.fetch_add(1, Relaxed);
        }
    }
}

#[cfg(not(tokio_internal_mt_counters))]
mod imp {
    pub(crate) fn inc_num_inc_notify_local() {}
    pub(crate) fn inc_num_unparks_local() {}
    pub(crate) fn inc_num_maintenance() {}
    pub(crate) fn inc_num_need_searchers() {}
    pub(crate) fn inc_num_defers(_batch: usize) {}
}

#[derive(Debug)]
pub(crate) struct Counters;

pub(super) use imp::*;
