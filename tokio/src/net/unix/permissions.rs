use crate::net::unix;

/// Permissions for a [`UnixSocket`].
///
/// Unlike [`std::fs::Permissions`], which can hold other platform-specific
/// state, this type holds only mode bits.
///
/// [`UnixSocket`]: crate::net::UnixSocket
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Permissions {
    mode: unix::mode_t,
}

impl Permissions {
    /// Creates permissions from the given mode bits.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::unix::Permissions;
    ///
    /// // Let only owner connect to socket.
    /// let permissions = Permissions::from_mode(0o600);
    /// assert_eq!(permissions.mode(), 0o600);
    ///
    /// // Let owner and group connect to socket.
    /// let permissions = Permissions::from_mode(0o660);
    /// assert_eq!(permissions.mode(), 0o660);
    ///
    /// // Let owner, group, and others connect to socket.
    /// let permissions = Permissions::from_mode(0o666);
    /// assert_eq!(permissions.mode(), 0o666);
    /// ```
    pub fn from_mode(mode: unix::mode_t) -> Permissions {
        Permissions { mode }
    }

    /// Gets the mode bits.
    pub fn mode(&self) -> unix::mode_t {
        self.mode
    }
}
