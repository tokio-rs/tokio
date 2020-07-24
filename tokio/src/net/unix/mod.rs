//! Unix domain socket utility types

pub mod datagram;

mod incoming;
pub use incoming::Incoming;

pub(crate) mod listener;
pub(crate) use listener::UnixListener;

mod split;
pub use split::{ReadHalf, WriteHalf};

mod split_owned;
pub use split_owned::{OwnedReadHalf, OwnedWriteHalf, ReuniteError};

pub(crate) mod stream;
pub(crate) use stream::UnixStream;

mod ucred;
pub use ucred::UCred;
