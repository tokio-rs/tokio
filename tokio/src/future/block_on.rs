use std::future::Future;

cfg_rt! {
    pub(crate) fn block_on<F: Future>(f: F) -> F::Output {
        // FIXME: Waiting for https://github.com/tokio-rs/tokio/pull/3097"
        // t10::runtime::Handle::current().block_on(f)

        let h = t10::runtime::Handle::current();
        let _enter = h.enter();
        futures::executor::block_on(f)
    }
}

cfg_not_rt! {
    pub(crate) fn block_on<F: Future>(_f: F) -> F::Output {
        // FIXME performance degradation?
       //futures::executor::block_on(f)
       todo!()
    }
}
