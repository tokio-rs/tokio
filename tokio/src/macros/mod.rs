#![cfg_attr(not(feature = "full"), allow(unused_macros))]

#[macro_use]
#[cfg(test)]
mod assert;

#[macro_use]
mod cfg;

#[macro_use]
mod join;

#[macro_use]
mod loom;

#[macro_use]
mod pin;

#[macro_use]
mod ready;

cfg_macros! {
    #[macro_use]
    mod select;
}

#[macro_use]
mod thread_local;

// Includes re-exports needed to implement macros
#[doc(hidden)]
pub mod support;
