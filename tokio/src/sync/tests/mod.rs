#[cfg(not(loom))]
mod atomic_waker;

#[cfg(loom)]
mod loom_atomic_waker;

#[cfg(loom)]
mod loom_list;

#[cfg(loom)]
mod loom_mpsc;

#[cfg(loom)]
mod loom_oneshot;

#[cfg(loom)]
mod loom_semaphore;
