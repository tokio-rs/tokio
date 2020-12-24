//! TCP utility types

pub(crate) mod listener;

pub(crate) mod socket;

//mod split;
pub use t10::net::tcp::{ReadHalf, WriteHalf};

//mod split_owned;
pub use t10::net::tcp::{OwnedReadHalf, OwnedWriteHalf, ReuniteError};

pub(crate) mod stream;
pub(crate) use stream::TcpStream;
