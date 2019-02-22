//! Future-aware synchronization
//!
//! This module is enabled with the **`sync`** feature flag.
//!
//! Tasks sometimes need to communicate with each other. This module contains
//! two basic abstractions for doing so:
//!
//! - [oneshot](oneshot/index.html), a way of sending a single value
//!   from one task to another.
//! - [mpsc](mpsc/index.html), a multi-producer, single-consumer channel for
//!   sending values between tasks.

pub use tokio_sync::{mpsc, oneshot};
