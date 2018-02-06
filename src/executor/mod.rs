//! Task execution utilities.
//!
//! This module only contains `current_thread`, an executor for multiplexing
//! many tasks on a single thread.

pub mod current_thread;
mod scheduler;
mod sleep;
