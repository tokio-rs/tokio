//! Stream utilities for Tokio.
//! 
//! `Stream`s are an asynchoronous version of standard library's Iterator.
//! 
//! This module provides helpers to work with them.

pub use futures_core::Stream;

mod iter;
pub use iter::{iter, Iter};

mod streamext;
pub use streamext::{StreamExt, Next, Map};
