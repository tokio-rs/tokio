use crate::runtime::scheduler;

#[track_caller]
pub(crate) fn block_in_place<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    #[cfg(tokio_unstable)]
    {
        use crate::runtime::{Handle, RuntimeFlavor::MultiThreadAlt};

        match Handle::try_current().map(|h| h.runtime_flavor()) {
            Ok(MultiThreadAlt) => {
                return scheduler::multi_thread_alt::block_in_place(f);
            }
            _ => {}
        }
    }

    scheduler::multi_thread::block_in_place(f)
}
