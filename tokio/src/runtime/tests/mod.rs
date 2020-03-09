cfg_loom! {
    mod loom_blocking;
    mod loom_oneshot;
    mod loom_pool;
}

#[cfg(miri)]
mod task;
