use std::future::Future;
use crate::runtime::context::BlockingRegionGuard;
use std::error::Error;

cfg_rt! {
    #[track_caller]
    pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
        let mut e = crate::runtime::context::try_enter_blocking_region().expect(
            "Cannot block the current thread from within a runtime. This \
            happens because a function attempted to block the current \
            thread while the thread is being used to drive asynchronous \
            tasks."
        );
        e.block_on(f).unwrap()
    }

    pub(crate) fn try_blocking_on<F: Future>(f: F) -> Result<F::Output, Box<dyn Error>> {
        match crate::runtime::context::try_enter_blocking_region(){
            Some(mut e) => Ok(e.block_on(f).unwrap()),
            None => Err("Cannot block the current thread from within a runtime. This \
            happens because a function attempted to block the current \
            thread while the thread is being used to drive asynchronous \
            tasks.".into())
        }
    }
}

cfg_not_rt! {
    #[track_caller]
    pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
        let mut park = crate::runtime::park::CachedParkThread::new();
        park.block_on(f).unwrap()
    }
}
