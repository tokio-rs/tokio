//! This module abstracts over `loom` and `std::sync` depending on whether we
//! are running tests or not.
//! This module is directly copied from tokio. Everything in this module is subject to the same license as tokio.

#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(not(loom))]
mod std;
#[cfg(not(loom))]
pub(crate) use self::std::*;

#[cfg(loom)]
mod mocked;
#[cfg(loom)]
pub(crate) use self::mocked::*;
