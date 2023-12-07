use std::fmt;
use std::path::Path;

/// An address associated with a Tokio Unix socket.
pub struct SocketAddr(pub(super) mio::net::SocketAddr);

impl SocketAddr {
    /// Returns `true` if the address is unnamed.
    ///
    /// Documentation reflected in [`SocketAddr`]
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn is_unnamed(&self) -> bool {
        self.0.is_unnamed()
    }

    /// Returns the contents of this address if it is a `pathname` address.
    ///
    /// Documentation reflected in [`SocketAddr`]
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn as_pathname(&self) -> Option<&Path> {
        self.0.as_pathname()
    }

    /// Returns the contents of this address if it is an abstract namespace.
    ///
    /// See also the standard library documentation on [`SocketAddr`].
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn as_abstract_namespace(&self) -> Option<&[u8]> {
        self.0.as_abstract_namespace()
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmt)
    }
}
