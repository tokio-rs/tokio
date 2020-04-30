cfg_not_loom! {
    mod atomic_waker;
    mod semaphore_ll;
    mod semaphore_batch;
}

cfg_loom! {
    mod loom_atomic_waker;
    mod loom_broadcast;
    #[cfg(tokio_unstable)]
    mod loom_cancellation_token;
    mod loom_list;
    mod loom_mpsc;
    mod loom_notify;
    mod loom_oneshot;
    mod loom_semaphore_batch;
    mod loom_semaphore_ll;
}
