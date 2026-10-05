use crate::net::unix;

/// Permissions for a Unix socket.
///
/// Unlike [`std::fs::Permissions`], which can hold other platform-specific
/// state, this type holds only mode bits.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct Permissions {
    mode: unix::mode_t,
}

impl Permissions {
    /// Creates permissions from the given mode bits.
    pub fn from_mode(mode: unix::mode_t) -> Permissions {
        Permissions { mode }
    }

    /// Gets the mode bits.
    pub fn mode(&self) -> unix::mode_t {
        self.mode
    }
}
