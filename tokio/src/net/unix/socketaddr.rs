use std::fmt;
use std::path::Path;

// helps rustdoc on non-supported platforms.
doc_prelude! {
    mod mock {
        pub(super) mod mio_net {
            pub struct SocketAddr(());
        }
    }

    #[cfg(unix)] {
        pub(super) use mio::net as mio_net;
    }
}

/// An address associated with a Tokio Unix socket.
pub struct SocketAddr(pub(super) mio_net::SocketAddr);

impl SocketAddr {
    /// Returns `true` if the address is unnamed.
    ///
    /// Documentation reflected in [`SocketAddr`]
    ///
    /// [`SocketAddr`]: crate::os::unix::net::SocketAddr
    pub fn is_unnamed(&self) -> bool {
        self.0.is_unnamed()
    }

    /// Returns the contents of this address if it is a `pathname` address.
    ///
    /// Documentation reflected in [`SocketAddr`]
    ///
    /// [`SocketAddr`]: crate::os::unix::net::SocketAddr
    pub fn as_pathname(&self) -> Option<&Path> {
        self.0.as_pathname()
    }
}

impl fmt::Debug for SocketAddr {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmt)
    }
}
