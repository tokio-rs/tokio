#![cfg_attr(not(feature = "full"), allow(unused_macros))]

#[macro_use]
mod cfg;

#[macro_use]
mod loom;

#[macro_use]
mod pin;

#[macro_use]
mod thread_local;

#[macro_use]
mod addr_of;

cfg_trace! {
    #[macro_use]
    mod trace;
}

cfg_macros! {
    #[macro_use]
    mod select;

    #[macro_use]
    mod join;

    #[macro_use]
    mod try_join;
}

// Includes re-exports needed to implement macros
#[doc(hidden)]
pub mod support;

#[doc(hidden)]
#[macro_export]
#[cfg(not(target_os = "emscripten"))]
macro_rules! __tokio_unsupported_multi_thread_on_emscripten {
    ($($body:tt)*) => { $($body)* };
}

#[doc(hidden)]
#[macro_export]
#[cfg(target_os = "emscripten")]
macro_rules! __tokio_unsupported_multi_thread_on_emscripten {
    ($($body:tt)*) => {
        ::core::compile_error!(
            "the `multi_thread` runtime flavor is not available on \
             wasm32-unknown-emscripten (no native threads); use \
             `flavor = \"current_thread\"`"
        );
    };
}
