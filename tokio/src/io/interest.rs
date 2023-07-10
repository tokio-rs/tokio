#![cfg_attr(not(feature = "net"), allow(dead_code, unreachable_pub))]

use crate::io::ready::Ready;

use std::fmt;
use std::ops;

/// Readiness event interest.
///
/// Specifies the readiness events the caller is interested in when awaiting on
/// I/O resource readiness states.
#[cfg_attr(docsrs, doc(cfg(feature = "net")))]
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Interest {
    mio: Option<mio::Interest>,
    has_error_interest: bool,
}

impl Interest {
    const fn new(mio: mio::Interest) -> Self {
        Self {
            mio: Some(mio),
            has_error_interest: false,
        }
    }

    // The non-FreeBSD definitions in this block are active only when
    // building documentation.
    cfg_aio! {
        /// Interest for POSIX AIO.
        #[cfg(target_os = "freebsd")]
        pub const AIO: Interest = Interest::new(mio::Interest::AIO);

        /// Interest for POSIX AIO.
        #[cfg(not(target_os = "freebsd"))]
        pub const AIO: Interest = Interest::new(mio::Interest::READABLE);

        /// Interest for POSIX AIO lio_listio events.
        #[cfg(target_os = "freebsd")]
        pub const LIO: Interest = Interest::new(mio::Interest::LIO);

        /// Interest for POSIX AIO lio_listio events.
        #[cfg(not(target_os = "freebsd"))]
        pub const LIO: Interest = Interest::new(mio::Interest::READABLE);
    }

    /// Interest in all readable events.
    ///
    /// Readable interest includes read-closed events.
    pub const READABLE: Interest = Interest::new(mio::Interest::READABLE);

    /// Interest in all writable events.
    ///
    /// Writable interest includes write-closed events.
    pub const WRITABLE: Interest = Interest::new(mio::Interest::WRITABLE);

    /// Interest in error events.
    pub const ERROR: Interest = Interest {
        mio: None,
        has_error_interest: true,
    };

    /// Returns a `Interest` set representing priority completion interests.
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[cfg_attr(docsrs, doc(cfg(any(target_os = "linux", target_os = "android"))))]
    pub const PRIORITY: Interest = Interest::new(mio::Interest::PRIORITY);

    /// Returns true if the value includes readable interest.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::Interest;
    ///
    /// assert!(Interest::READABLE.is_readable());
    /// assert!(!Interest::WRITABLE.is_readable());
    ///
    /// let both = Interest::READABLE | Interest::WRITABLE;
    /// assert!(both.is_readable());
    /// ```
    pub const fn is_readable(self) -> bool {
        match self.mio {
            Some(mio) => mio.is_readable(),
            None => false,
        }
    }

    /// Returns true if the value includes writable interest.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::Interest;
    ///
    /// assert!(!Interest::READABLE.is_writable());
    /// assert!(Interest::WRITABLE.is_writable());
    ///
    /// let both = Interest::READABLE | Interest::WRITABLE;
    /// assert!(both.is_writable());
    /// ```
    pub const fn is_writable(self) -> bool {
        match self.mio {
            Some(mio) => mio.is_writable(),
            None => false,
        }
    }

    /// Returns true if the value includes priority interest.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::Interest;
    ///
    /// assert!(!Interest::READABLE.is_priority());
    /// assert!(Interest::PRIORITY.is_priority());
    ///
    /// let both = Interest::READABLE | Interest::PRIORITY;
    /// assert!(both.is_priority());
    /// ```
    #[cfg(any(target_os = "linux", target_os = "android"))]
    #[cfg_attr(docsrs, doc(cfg(any(target_os = "linux", target_os = "android"))))]
    pub const fn is_priority(self) -> bool {
        match self.mio {
            Some(mio) => mio.is_priority(),
            None => false,
        }
    }

    /// Add together two `Interest` values.
    ///
    /// This function works from a `const` context.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::Interest;
    ///
    /// const BOTH: Interest = Interest::READABLE.add(Interest::WRITABLE);
    ///
    /// assert!(BOTH.is_readable());
    /// assert!(BOTH.is_writable());
    pub const fn add(self, other: Interest) -> Interest {
        Interest {
            mio: match (self.mio, other.mio) {
                (Some(a), Some(b)) => Some(a.add(b)),
                (Some(c), None) | (None, Some(c)) => Some(c),
                (None, None) => None,
            },
            has_error_interest: self.has_error_interest || other.has_error_interest,
        }
    }

    /// Returns true if the value includes error interest.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::io::Interest;
    ///
    /// assert!(Interest::ERROR.is_error());
    /// assert!(!Interest::WRITABLE.is_error());
    ///
    /// let combined = Interest::READABLE | Interest::ERROR;
    /// assert!(combined.is_error());
    /// ```
    pub const fn is_error(self) -> bool {
        self.has_error_interest
    }

    // This function must be crate-private to avoid exposing a `mio` dependency.
    pub(crate) const fn to_mio(self) -> mio::Interest {
        match self.mio {
            Some(mio) => mio,
            None => {
                // error interest is implicit, and mio does not have a specific Interest::ERROR
                // so we provide an arbitrary interest here, and filter out the this readiness
                // in `Interest::mask` and `Ready::from_interest`.
                mio::Interest::READABLE
            }
        }
    }

    pub(crate) fn mask(self) -> Ready {
        match self {
            Interest::READABLE => Ready::READABLE | Ready::READ_CLOSED,
            Interest::WRITABLE => Ready::WRITABLE | Ready::WRITE_CLOSED,
            #[cfg(any(target_os = "linux", target_os = "android"))]
            Interest::PRIORITY => Ready::PRIORITY | Ready::READ_CLOSED,
            Interest::ERROR => Ready::ERROR,
            _ => Ready::EMPTY,
        }
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
        *self = *self | other
    }
}

impl fmt::Debug for Interest {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.mio {
            None => {
                if self.has_error_interest {
                    write!(fmt, "ERROR")
                } else {
                    Ok(())
                }
            }
            Some(mio) => {
                mio.fmt(fmt)?;
                if self.has_error_interest {
                    write!(fmt, " | ERROR")
                } else {
                    Ok(())
                }
            }
        }
    }
}
