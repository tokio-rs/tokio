//! UDP utility types.

pub(crate) mod socket;
mod split;

pub use split::{RecvHalf, SendHalf};

mod split_owned;
pub use split_owned::{OwnedRecvHalf, OwnedSendHalf};
