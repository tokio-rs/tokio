//! Unix domain socket utility types

pub mod datagram;

pub(crate) mod listener;

mod split;
pub use split::{ReadHalf, WriteHalf};

mod split_owned;
pub use split_owned::{OwnedReadHalf, OwnedWriteHalf, ReuniteError};

mod socketaddr;
pub use socketaddr::SocketAddr;

pub(crate) mod stream;
pub(crate) use stream::UnixStream;

mod ucred;
pub use ucred::UCred;
