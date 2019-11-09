//! Unix domain socket utility types

pub(crate) mod datagram;

mod incoming;
pub use incoming::Incoming;

pub(crate) mod listener;
pub(crate) use listener::UnixListener;

mod split;
pub use split::{ReadHalf, WriteHalf, RecvHalf, SendHalf};

pub(crate) mod stream;
pub(crate) use stream::UnixStream;

mod ucred;
pub use ucred::UCred;
