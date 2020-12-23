use std::future::Future;

cfg_rt! {
    pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
        t10::runtime::Handle::current().block_on(f)
    }
}

cfg_not_rt! {
    pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
        // FIXME performance degradation?
       futures::executor::block_on(f)
    }
}
