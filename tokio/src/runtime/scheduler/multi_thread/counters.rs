#[cfg(tokio_mt_counters)]
mod imp {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::Relaxed;

    static NUM_NOTIFY_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_UNPARKS_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_NEED_SEARCHERS: AtomicUsize = AtomicUsize::new(0);

    impl Drop for super::Counters {
        fn drop(&mut self) {
            println!("---");
            println!(
                "NUM_NOTIFY:         {:>10}",
                NUM_NOTIFY_LOCAL.load(Relaxed)
            );
            println!(
                "NUM_UNPARKS:        {:>10}",
                NUM_UNPARKS_LOCAL.load(Relaxed)
            );
            println!(
                "NUM_NEED_SEARCHERS: {:>10}",
                NUM_NEED_SEARCHERS.load(Relaxed)
            );
        }
    }

    pub(crate) fn inc_num_need_searchers() {
        NUM_NEED_SEARCHERS.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_inc_notify_local() {
        NUM_NOTIFY_LOCAL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_unparks_local() {
        NUM_UNPARKS_LOCAL.fetch_add(1, Relaxed);
    }
}

#[cfg(not(tokio_mt_counters))]
mod imp {
    pub(crate) fn inc_num_need_searchers() {}
    pub(crate) fn inc_num_inc_notify_local() {}
    pub(crate) fn inc_num_unparks_local() {}
}

#[derive(Debug)]
pub(crate) struct Counters;

pub(super) use imp::*;
