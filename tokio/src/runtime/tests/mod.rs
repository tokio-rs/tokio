cfg_loom! {
    mod loom_basic_scheduler;
    mod loom_blocking;
    mod loom_oneshot;
    mod loom_pool;
    mod loom_queue;
    mod loom_shutdown_join;
}

cfg_not_loom! {
    mod queue;

    #[cfg(miri)]
    mod task;
}
