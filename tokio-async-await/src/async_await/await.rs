/// Wait for a future to complete.
#[macro_export]
macro_rules! await {
    ($e:expr) => {{
        use $crate::std_await;
        #[allow(unused_imports)]
        use $crate::async_await::compat::forward::IntoAwaitable as IntoAwaitableForward;
        #[allow(unused_imports)]
        use $crate::async_await::compat::backward::IntoAwaitable as IntoAwaitableBackward;

        #[allow(unused_mut)]
        let mut e = $e;
        let e = e.into_awaitable();
        std_await!(e)
    }}
}
