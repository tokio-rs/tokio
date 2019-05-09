/// Wait for a future to complete.
#[macro_export]
macro_rules! await {
    ($e:expr) => {{
        #[allow(unused_imports)]
        use $crate::compat::backward::IntoAwaitable as IntoAwaitableBackward;
        #[allow(unused_imports)]
        use $crate::compat::forward::IntoAwaitable as IntoAwaitableForward;
        use $crate::std_await;

        #[allow(unused_mut)]
        let mut e = $e;
        let e = e.into_awaitable();
        // TODO: The code that the macro expands inherits the edition of the crate in which the macro is defined.
        // When this crate transitioned to 2018 edition, replace it with the `await` syntax.
        std_await!(e)
    }};
}
