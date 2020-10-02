use std::fmt;
use std::ops;

const READABLE: usize = 0b0_01;
const WRITABLE: usize = 0b0_10;
const READ_CLOSED: usize = 0b0_0100;
const WRITE_CLOSED: usize = 0b0_1000;

/// A set of readiness event kinds.
///
/// `Ready` is set of operation descriptors indicating which kind of an
/// operation is ready to be performed.
///
/// This struct only represents portable event kinds. Portable events are
/// events that can be raised on any platform while guaranteeing no false
/// positives.
#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub(crate) struct Ready(usize);

impl Ready {
    /// Returns the empty `Ready` set.
    pub(crate) const EMPTY: Ready = Ready(0);

    /// Returns a `Ready` representing readable readiness.
    pub(crate) const READABLE: Ready = Ready(READABLE);

    /// Returns a `Ready` representing writable readiness.
    pub(crate) const WRITABLE: Ready = Ready(WRITABLE);

    /// Returns a `Ready` representing read closed readiness.
    pub(crate) const READ_CLOSED: Ready = Ready(READ_CLOSED);

    /// Returns a `Ready` representing write closed readiness.
    pub(crate) const WRITE_CLOSED: Ready = Ready(WRITE_CLOSED);

    /// Returns a `Ready` representing readiness for all operations.
    pub(crate) const ALL: Ready = Ready(READABLE | WRITABLE | READ_CLOSED | WRITE_CLOSED);

    pub(crate) fn from_mio(event: &mio::event::Event) -> Ready {
        let mut ready = Ready::EMPTY;

        if event.is_readable() {
            ready |= Ready::READABLE;
        }

        if event.is_writable() {
            ready |= Ready::WRITABLE;
        }

        if event.is_read_closed() {
            ready |= Ready::READ_CLOSED;
        }

        if event.is_write_closed() {
            ready |= Ready::WRITE_CLOSED;
        }

        ready
    }

    /// Returns true if `Ready` is the empty set
    pub(crate) fn is_empty(self) -> bool {
        self == Ready::EMPTY
    }

    /// Returns true if the value includes readable readiness
    pub(crate) fn is_readable(self) -> bool {
        self.contains(Ready::READABLE) || self.is_read_closed()
    }

    /// Returns true if the value includes writable readiness
    pub(crate) fn is_writable(self) -> bool {
        self.contains(Ready::WRITABLE) || self.is_write_closed()
    }

    /// Returns true if the value includes read closed readiness
    pub(crate) fn is_read_closed(self) -> bool {
        self.contains(Ready::READ_CLOSED)
    }

    /// Returns true if the value includes write closed readiness
    pub(crate) fn is_write_closed(self) -> bool {
        self.contains(Ready::WRITE_CLOSED)
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// `other` may represent more than one readiness operations, in which case
    /// the function only returns true if `self` contains all readiness
    /// specified in `other`.
    pub(crate) fn contains<T: Into<Self>>(self, other: T) -> bool {
        let other = other.into();
        (self & other) == other
    }

    /// Create a `Ready` instance using the given `usize` representation.
    ///
    /// The `usize` representation must have been obtained from a call to
    /// `Readiness::as_usize`.
    ///
    /// This function is mainly provided to allow the caller to get a
    /// readiness value from an `AtomicUsize`.
    pub(crate) fn from_usize(val: usize) -> Ready {
        Ready(val & Ready::ALL.as_usize())
    }

    /// Returns a `usize` representation of the `Ready` value.
    ///
    /// This function is mainly provided to allow the caller to store a
    /// readiness value in an `AtomicUsize`.
    pub(crate) fn as_usize(self) -> usize {
        self.0
    }
}

cfg_io_readiness! {
    impl Ready {
        pub(crate) fn from_interest(interest: mio::Interest) -> Ready {
            let mut ready = Ready::EMPTY;

            if interest.is_readable() {
                ready |= Ready::READABLE;
                ready |= Ready::READ_CLOSED;
            }

            if interest.is_writable() {
                ready |= Ready::WRITABLE;
                ready |= Ready::WRITE_CLOSED;
            }

            ready
        }

        pub(crate) fn intersection(self, interest: mio::Interest) -> Ready {
            Ready(self.0 & Ready::from_interest(interest).0)
        }

        pub(crate) fn satisfies(self, interest: mio::Interest) -> bool {
            self.0 & Ready::from_interest(interest).0 != 0
        }
    }
}

impl<T: Into<Ready>> ops::BitOr<T> for Ready {
    type Output = Ready;

    #[inline]
    fn bitor(self, other: T) -> Ready {
        Ready(self.0 | other.into().0)
    }
}

impl<T: Into<Ready>> ops::BitOrAssign<T> for Ready {
    #[inline]
    fn bitor_assign(&mut self, other: T) {
        self.0 |= other.into().0;
    }
}

impl<T: Into<Ready>> ops::BitAnd<T> for Ready {
    type Output = Ready;

    #[inline]
    fn bitand(self, other: T) -> Ready {
        Ready(self.0 & other.into().0)
    }
}

impl<T: Into<Ready>> ops::Sub<T> for Ready {
    type Output = Ready;

    #[inline]
    fn sub(self, other: T) -> Ready {
        Ready(self.0 & !other.into().0)
    }
}

impl fmt::Debug for Ready {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("Ready")
            .field("is_readable", &self.is_readable())
            .field("is_writable", &self.is_writable())
            .field("is_read_closed", &self.is_read_closed())
            .field("is_write_closed", &self.is_write_closed())
            .finish()
    }
}
