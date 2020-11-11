use std::fmt;
use std::ops;

/// Readiness event interest
///
/// Specifies the readiness events the caller is interested in when awaiting on
/// I/O resource readiness states.
#[derive(Clone, Copy)]
pub(crate) struct Interest(mio::Interest);

impl Interest {
    /// Interest in all readable events
    pub(crate) const READABLE: Interest = Interest(mio::Interest::READABLE);

    /// Interest in all writable events
    pub(crate) const WRITABLE: Interest = Interest(mio::Interest::WRITABLE);

    /// Returns true if the value includes readable interest.
    pub(crate) const fn is_readable(self) -> bool {
        self.0.is_readable()
    }

    /// Returns true if the value includes writable interest.
    pub(crate) const fn is_writable(self) -> bool {
        self.0.is_writable()
    }

    /// Add together two `Interst` values.
    pub(crate) const fn add(self, other: Interest) -> Interest {
        Interest(self.0.add(other.0))
    }

    pub(crate) const fn to_mio(self) -> mio::Interest {
        self.0
    }
}

impl ops::BitOr for Interest {
    type Output = Self;

    #[inline]
    fn bitor(self, other: Self) -> Self {
        self.add(other)
    }
}

impl ops::BitOrAssign for Interest {
    #[inline]
    fn bitor_assign(&mut self, other: Self) {
        self.0 = (*self | other).0;
    }
}

impl fmt::Debug for Interest {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(fmt)
    }
}
