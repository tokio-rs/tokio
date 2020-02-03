cfg_not_loom! {
    mod atomic_waker;
    mod semaphore_ll;
}

cfg_loom! {
    mod loom_atomic_waker;
    mod loom_broadcast;
    mod loom_list;
    mod loom_mpsc;
    mod loom_notify;
    mod loom_oneshot;
    mod loom_semaphore_ll;
}
