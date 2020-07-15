cfg_loom! {
    mod loom_blocking;
    mod loom_oneshot;
    mod loom_pool;
    mod loom_queue;
}

cfg_not_loom! {
    mod queue;

    #[cfg(miri)]
    mod task;
}
