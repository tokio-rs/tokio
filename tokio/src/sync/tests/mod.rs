cfg_not_loom! {
    mod atomic_waker;
    mod semaphore;
}

cfg_loom! {
    mod loom_atomic_waker;
    mod loom_list;
    mod loom_mpsc;
    mod loom_oneshot;
    mod loom_semaphore;
}
