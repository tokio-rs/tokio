/// Wait for a future to complete.
#[macro_export]
macro_rules! async_wait {
    ($e:expr) => {{
        #[allow(unused_imports)]
        use $crate::compat::backward::IntoAwaitable as IntoAwaitableBackward;
        #[allow(unused_imports)]
        use $crate::compat::forward::IntoAwaitable as IntoAwaitableForward;

        #[allow(unused_mut)]
        let mut e = $e;
        let e = e.into_awaitable();
        e.await
    }};
}
