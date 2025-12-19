use std::fmt;
use std::io;
#[cfg(target_os = "android")]
use std::os::android::net::SocketAddrExt;
#[cfg(target_os = "linux")]
use std::os::linux::net::SocketAddrExt;
#[cfg(any(target_os = "linux", target_os = "android"))]
use std::os::unix::ffi::OsStrExt;
use std::os::unix::net::SocketAddr as StdSocketAddr;
use std::path::Path;

/// An address associated with a Tokio Unix socket.
///
/// This type is a thin wrapper around [`std::os::unix::net::SocketAddr`]. You
/// can convert to and from the standard library `SocketAddr` type using the
/// [`From`] trait.
#[derive(Clone)]
pub struct SocketAddr(pub(super) StdSocketAddr);

impl SocketAddr {
    /// Creates an address from a path
    ///
    /// On Linux, allows the path to start with a leading `\0` to declare
    /// a socket path within the abstract namespace.
    pub fn from_path<P>(path: P) -> io::Result<Self>
    where
        P: AsRef<Path>,
    {
        // For now, we handle abstract socket paths on linux here.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        let addr = {
            let os_str_bytes = path.as_ref().as_os_str().as_bytes();
            if os_str_bytes.starts_with(b"\0") {
                StdSocketAddr::from_abstract_name(&os_str_bytes[1..])?
            } else {
                StdSocketAddr::from_pathname(path)?
            }
        };
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        let addr = StdSocketAddr::from_pathname(path)?;

        Ok(SocketAddr(addr))
    }

    /// Returns `true` if the address is unnamed.
    ///
    /// Documentation reflected in [`SocketAddr`].
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn is_unnamed(&self) -> bool {
        self.0.is_unnamed()
    }

    /// Returns the contents of this address if it is a `pathname` address.
    ///
    /// Documentation reflected in [`SocketAddr`].
    ///
    /// [`SocketAddr`]: std::os::unix::net::SocketAddr
    pub fn as_pathname(&self) -> Option<&Path> {
        self.0.as_pathname()
    }

    /// Returns the contents of this address if it is in the abstract namespace.
    ///
    /// Documentation reflected in [`SocketAddrExt`].
    /// The abstract namespace is a Linux-specific feature.
    ///
    ///
    /// [`SocketAddrExt`]: std::os::linux::net::SocketAddrExt
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[cfg_attr(docsrs, doc(cfg(any(target_os = "linux", target_os = "android"))))]
    pub fn as_abstract_name(&self) -> Option<&[u8]> {
        #[cfg(target_os = "android")]
        use std::os::android::net::SocketAddrExt;
        #[cfg(target_os = "linux")]
        use std::os::linux::net::SocketAddrExt;

        self.0.as_abstract_name()
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
