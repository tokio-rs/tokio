#![cfg_attr(not(feature = "full"), allow(unused_macros))]

#[macro_use]
mod cfg;

#[macro_use]
mod loom;

#[macro_use]
mod pin;

#[macro_use]
mod ready;

#[macro_use]
mod thread_local;

#[macro_use]
#[cfg(feature = "rt-core")]
pub(crate) mod scoped_tls;

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
