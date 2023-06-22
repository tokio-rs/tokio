#[cfg(tokio_internal_mt_counters)]
mod imp {
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering::Relaxed;

    static NUM_MAINTENANCE: AtomicUsize = AtomicUsize::new(0);
    static NUM_NOTIFY_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_NOTIFY_REMOTE: AtomicUsize = AtomicUsize::new(0);
    static NUM_UNPARKS_LOCAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_UNPARKS_REMOTE: AtomicUsize = AtomicUsize::new(0);
    static NUM_LIFO_SCHEDULES: AtomicUsize = AtomicUsize::new(0);
    static NUM_LIFO_CAPPED: AtomicUsize = AtomicUsize::new(0);
    static NUM_STEALS: AtomicUsize = AtomicUsize::new(0);
    static NUM_OVERFLOW: AtomicUsize = AtomicUsize::new(0);
    static NUM_PARK: AtomicUsize = AtomicUsize::new(0);
    static NUM_POLLS: AtomicUsize = AtomicUsize::new(0);
    static NUM_LIFO_POLLS: AtomicUsize = AtomicUsize::new(0);
    static NUM_REMOTE_BATCH: AtomicUsize = AtomicUsize::new(0);
    static NUM_GLOBAL_QUEUE_INTERVAL: AtomicUsize = AtomicUsize::new(0);
    static NUM_NO_AVAIL_CORE: AtomicUsize = AtomicUsize::new(0);
    static NUM_RELAY_SEARCH: AtomicUsize = AtomicUsize::new(0);
    static NUM_SPIN_STALL: AtomicUsize = AtomicUsize::new(0);
    static NUM_NO_LOCAL_WORK: AtomicUsize = AtomicUsize::new(0);

    impl Drop for super::Counters {
        fn drop(&mut self) {
            let notifies_local = NUM_NOTIFY_LOCAL.load(Relaxed);
            let notifies_remote = NUM_NOTIFY_REMOTE.load(Relaxed);
            let unparks_local = NUM_UNPARKS_LOCAL.load(Relaxed);
            let unparks_remote = NUM_UNPARKS_REMOTE.load(Relaxed);
            let maintenance = NUM_MAINTENANCE.load(Relaxed);
            let lifo_scheds = NUM_LIFO_SCHEDULES.load(Relaxed);
            let lifo_capped = NUM_LIFO_CAPPED.load(Relaxed);
            let num_steals = NUM_STEALS.load(Relaxed);
            let num_overflow = NUM_OVERFLOW.load(Relaxed);
            let num_park = NUM_PARK.load(Relaxed);
            let num_polls = NUM_POLLS.load(Relaxed);
            let num_lifo_polls = NUM_LIFO_POLLS.load(Relaxed);
            let num_remote_batch = NUM_REMOTE_BATCH.load(Relaxed);
            let num_global_queue_interval = NUM_GLOBAL_QUEUE_INTERVAL.load(Relaxed);
            let num_no_avail_core = NUM_NO_AVAIL_CORE.load(Relaxed);
            let num_relay_search = NUM_RELAY_SEARCH.load(Relaxed);
            let num_spin_stall = NUM_SPIN_STALL.load(Relaxed);
            let num_no_local_work = NUM_NO_LOCAL_WORK.load(Relaxed);

            println!("---");
            println!("notifies (remote): {}", notifies_remote);
            println!(" notifies (local): {}", notifies_local);
            println!("  unparks (local): {}", unparks_local);
            println!(" unparks (remote): {}", unparks_remote);
            println!("  notify, no core: {}", num_no_avail_core);
            println!("      maintenance: {}", maintenance);
            println!("   LIFO schedules: {}", lifo_scheds);
            println!("      LIFO capped: {}", lifo_capped);
            println!("           steals: {}", num_steals);
            println!("  queue overflows: {}", num_overflow);
            println!("            parks: {}", num_park);
            println!("            polls: {}", num_polls);
            println!("     polls (LIFO): {}", num_lifo_polls);
            println!("remote task batch: {}", num_remote_batch);
            println!("global Q interval: {}", num_global_queue_interval);
            println!("     relay search: {}", num_relay_search);
            println!("       spin stall: {}", num_spin_stall);
            println!("    no local work: {}", num_no_local_work);
        }
    }

    pub(crate) fn inc_num_inc_notify_local() {
        NUM_NOTIFY_LOCAL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_notify_remote() {
        NUM_NOTIFY_REMOTE.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_unparks_local() {
        NUM_UNPARKS_LOCAL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_unparks_remote() {
        NUM_UNPARKS_REMOTE.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_maintenance() {
        NUM_MAINTENANCE.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_lifo_schedules() {
        NUM_LIFO_SCHEDULES.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_lifo_capped() {
        NUM_LIFO_CAPPED.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_steals() {
        NUM_STEALS.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_overflows() {
        NUM_OVERFLOW.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_parks() {
        NUM_PARK.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_polls() {
        NUM_POLLS.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_lifo_polls() {
        NUM_LIFO_POLLS.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_remote_batch() {
        NUM_REMOTE_BATCH.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_global_queue_interval() {
        NUM_GLOBAL_QUEUE_INTERVAL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_notify_no_core() {
        NUM_NO_AVAIL_CORE.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_relay_search() {
        NUM_RELAY_SEARCH.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_spin_stall() {
        NUM_SPIN_STALL.fetch_add(1, Relaxed);
    }

    pub(crate) fn inc_num_no_local_work() {
        NUM_NO_LOCAL_WORK.fetch_add(1, Relaxed);
    }
}

#[cfg(not(tokio_internal_mt_counters))]
mod imp {
    pub(crate) fn inc_num_inc_notify_local() {}
    pub(crate) fn inc_num_notify_remote() {}
    pub(crate) fn inc_num_unparks_local() {}
    pub(crate) fn inc_num_unparks_remote() {}
    pub(crate) fn inc_num_maintenance() {}
    pub(crate) fn inc_lifo_schedules() {}
    pub(crate) fn inc_lifo_capped() {}
    pub(crate) fn inc_num_steals() {}
    pub(crate) fn inc_num_overflows() {}
    pub(crate) fn inc_num_parks() {}
    pub(crate) fn inc_num_polls() {}
    pub(crate) fn inc_num_lifo_polls() {}
    pub(crate) fn inc_num_remote_batch() {}
    pub(crate) fn inc_global_queue_interval() {}
    pub(crate) fn inc_notify_no_core() {}
    pub(crate) fn inc_num_relay_search() {}
    pub(crate) fn inc_num_spin_stall() {}
    pub(crate) fn inc_num_no_local_work() {}
}

#[derive(Debug)]
pub(crate) struct Counters;

pub(super) use imp::*;
