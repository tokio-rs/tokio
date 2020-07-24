//! Unix datagram types.

pub(crate) mod socket;
pub(crate) mod split;
pub(crate) mod split_owned;

pub use split::{RecvHalf, SendHalf};
pub use split_owned::{OwnedRecvHalf, OwnedSendHalf, ReuniteError};
