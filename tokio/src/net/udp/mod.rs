//! UDP utility types.

pub(crate) mod socket;
pub(crate) use socket::UdpSocket;

mod split;
pub use split::{RecvHalf, ReuniteError, SendHalf};
