//! UDP framing
#![cfg(not(loom))]
mod frame;
pub use frame::UdpFramed;
