#![cfg_attr(not(feature = "full"), allow(unused_macros))]

#[macro_use]
#[cfg(test)]
mod assert;

#[macro_use]
mod cfg;

#[macro_use]
mod loom;

#[macro_use]
mod ready;

#[macro_use]
mod select;

#[macro_use]
mod thread_local;

cfg_macros! {
    // Includes re-exports needed to implement macros
    #[doc(hidden)]
    pub mod support;
}
