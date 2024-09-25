use std::fmt;
use std::path::Path;

/// An address associated with a Tokio Unix socket.
///
/// This type is a think wrapper around
/// [`std::os::unix::net::SocketAddr`];
/// you can use [`From`] to wrap and unwrap
/// instances of [`std::os::unix::net::SocketAddr`] as this type.
pub struct SocketAddr(pub(super) std::os::unix::net::SocketAddr);

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
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmt)
    }
}

impl From<std::os::unix::net::SocketAddr> for SocketAddr {
    fn from(value: std::os::unix::net::SocketAddr) -> Self {
        SocketAddr(value)
    }
}

impl From<SocketAddr> for std::os::unix::net::SocketAddr {
    fn from(value: SocketAddr) -> Self {
        value.0
    }
}
