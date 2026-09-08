cfg_rt! {
    #[cfg(any(not(target_os = "emscripten"), target_feature = "atomics"))]
    pub(crate) use crate::runtime::spawn_blocking;

    cfg_fs! {
        #[cfg(any(not(target_os = "emscripten"), target_feature = "atomics"))]
        #[allow(unused_imports)]
        pub(crate) use crate::runtime::spawn_mandatory_blocking;
    }

    #[cfg(any(not(target_os = "emscripten"), target_feature = "atomics"))]
    pub(crate) use crate::task::JoinHandle;

    // Non-pthread emscripten has no blocking pool, and the `std` calls behind
    // `fs` and `io-std` complete synchronously there, so this internal shim
    // runs the closure inline and hands back an already-completed future. The
    // public `task::spawn_blocking` is not routed through here and keeps its
    // native semantics. Pthread builds (`+atomics`) use the native pool.
    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
    pub(crate) type JoinHandle<T> = std::future::Ready<Result<T, crate::task::JoinError>>;

    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics")))]
    pub(crate) fn spawn_blocking<F, R>(f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        std::future::ready(Ok(f()))
    }

    #[cfg(all(target_os = "emscripten", not(target_feature = "atomics"), feature = "fs"))]
    #[allow(dead_code)] // unit tests replace this with the `fs::mocks` version
    pub(crate) fn spawn_mandatory_blocking<F, R>(f: F) -> Option<JoinHandle<R>>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        Some(spawn_blocking(f))
    }
}

cfg_not_rt! {
    use std::fmt;
    use std::future::Future;
    use std::pin::Pin;
    use std::task::{Context, Poll};

    pub(crate) fn spawn_blocking<F, R>(_f: F) -> JoinHandle<R>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        assert_send_sync::<JoinHandle<std::cell::Cell<()>>>();
        panic!("requires the `rt` Tokio feature flag")
    }

    cfg_fs! {
        pub(crate) fn spawn_mandatory_blocking<F, R>(_f: F) -> Option<JoinHandle<R>>
        where
            F: FnOnce() -> R + Send + 'static,
            R: Send + 'static,
        {
            panic!("requires the `rt` Tokio feature flag")
        }
    }

    pub(crate) struct JoinHandle<R> {
        _p: std::marker::PhantomData<R>,
    }

    unsafe impl<T: Send> Send for JoinHandle<T> {}
    unsafe impl<T: Send> Sync for JoinHandle<T> {}

    impl<R> Future for JoinHandle<R> {
        type Output = Result<R, std::io::Error>;

        fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            unreachable!()
        }
    }

    impl<T> fmt::Debug for JoinHandle<T>
    where
        T: fmt::Debug,
    {
        fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
            fmt.debug_struct("JoinHandle").finish()
        }
    }

    fn assert_send_sync<T: Send + Sync>() {
    }
}
