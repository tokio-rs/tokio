#[cfg(loom)]
mod loom_pool;

#[cfg(loom)]
mod loom_queue;

#[cfg(not(loom))]
mod queue;
