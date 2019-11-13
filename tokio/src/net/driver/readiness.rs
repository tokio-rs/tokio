use std::ops;

const READABLE: usize = 0b0_01;
const WRITABLE: usize = 0b0_10;
const READ_CLOSED: usize = 0b0_0100;
const WRITE_CLOSED: usize = 0b0_1000;

/// A set of readiness event kinds.
///
/// `Readiness` is set of operation descriptors indicating which kind of an
/// operation is ready to be performed.
///
/// This struct only represents portable event kinds. Portable events are
/// events that can be raised on any platform while guaranteeing no false
/// positives.
///
/// # Examples
///
/// ```rust
/// use tokio::net::driver::Readiness;
///
/// let readiness = Readiness::READABLE | Readiness::READ_CLOSED;
/// assert!(readiness.is_readable());
/// assert!(readiness.is_read_closed());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Readiness(usize);

impl Readiness {
    /// Returns the empty `Readiness` set.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::EMPTY;
    ///
    /// assert!(!readiness.is_readable());
    /// ```
    pub const EMPTY: Readiness = Readiness(0);

    /// Returns a `Readiness` representing readable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE;
    ///
    /// assert!(readiness.is_readable());
    /// ```
    pub const READABLE: Readiness = Readiness(READABLE);

    /// Returns a `Readiness` representing writable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::WRITABLE;
    ///
    /// assert!(readiness.is_writable());
    /// ```
    pub const WRITABLE: Readiness = Readiness(WRITABLE);

    /// Returns a `Readiness` representing read closed readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READ_CLOSED;
    ///
    /// assert!(readiness.is_read_closed());
    /// ```
    pub const READ_CLOSED: Readiness = Readiness(READ_CLOSED);

    /// Returns a `Readiness` representing write closed readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::WRITE_CLOSED;
    ///
    /// assert!(readiness.is_write_closed());
    /// ```
    pub const WRITE_CLOSED: Readiness = Readiness(WRITE_CLOSED);

    /// Returns a `Readiness` representing readiness for all operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::ALL;
    ///
    /// assert!(readiness.is_readable());
    /// assert!(readiness.is_writable());
    /// assert!(readiness.is_read_closed());
    /// assert!(readiness.is_write_closed());
    /// ```
    pub const ALL: Readiness = Readiness(READABLE | WRITABLE | READ_CLOSED | WRITE_CLOSED);

    /// Returns true if `Readiness` is the empty set
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::EMPTY;
    /// assert!(readiness.is_empty());
    /// ```
    pub fn is_empty(self) -> bool {
        self == Readiness::EMPTY
    }

    /// Returns true if the value includes readable readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE;
    ///
    /// assert!(readiness.is_readable());
    /// ```
    pub fn is_readable(self) -> bool {
        self.contains(Readiness::READABLE)
    }

    /// Returns true if the value includes writable readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::WRITABLE;
    ///
    /// assert!(readiness.is_writable());
    /// ```
    pub fn is_writable(self) -> bool {
        self.contains(Readiness::WRITABLE)
    }

    /// Returns true if the value includes read closed readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READ_CLOSED;
    ///
    /// assert!(readiness.is_read_closed());
    /// ```
    pub fn is_read_closed(self) -> bool {
        self.contains(Readiness::READ_CLOSED)
    }

    /// Returns true if the value includes write closed readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::WRITE_CLOSED;
    ///
    /// assert!(readiness.is_write_closed());
    /// ```
    pub fn is_write_closed(self) -> bool {
        self.contains(Readiness::WRITE_CLOSED)
    }

    /// Returns true if `self` is a superset of `other`.
    ///
    /// `other` may represent more than one readiness operations, in which case
    /// the function only returns true if `self` contains all readiness
    /// specified in `other`.
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE;
    ///
    /// assert!(readiness.contains(Readiness::READABLE));
    /// assert!(!readiness.contains(Readiness::WRITABLE));
    /// ```
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE | Readiness::WRITABLE;
    ///
    /// assert!(readiness.contains(Readiness::READABLE));
    /// assert!(readiness.contains(Readiness::WRITABLE));
    /// ```
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE | Readiness::WRITABLE;
    ///
    /// assert!(!Readiness::READABLE.contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// ```
    pub fn contains<T: Into<Self>>(self, other: T) -> bool {
        let other = other.into();
        (self & other) == other
    }

    /// Create a `Readiness` instance using the given `usize` representation.
    ///
    /// The `usize` representation must have been obtained from a call to
    /// `Readiness::as_usize`.
    ///
    /// This function is mainly provided to allow the caller to get a
    /// readiness value from an `AtomicUsize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE;
    /// let readiness_usize = readiness.as_usize();
    /// let readiness2 = Readiness::from_usize(readiness_usize);
    ///
    /// assert_eq!(readiness, readiness2);
    /// ```
    pub fn from_usize(val: usize) -> Readiness {
        Readiness(val)
    }

    /// Returns a `usize` representation of the `Readiness` value.
    ///
    /// This function is mainly provided to allow the caller to store a
    /// readiness value in an `AtomicUsize`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::READABLE;
    /// let readiness_usize = readiness.as_usize();
    /// let readiness2 = Readiness::from_usize(readiness_usize);
    ///
    /// assert_eq!(readiness, readiness2);
    /// ```
    pub fn as_usize(self) -> usize {
        self.0
    }
}

impl<T: Into<Readiness>> ops::BitOr<T> for Readiness {
    type Output = Readiness;

    #[inline]
    fn bitor(self, other: T) -> Readiness {
        Readiness(self.0 | other.into().0)
    }
}

impl<T: Into<Readiness>> ops::BitOrAssign<T> for Readiness {
    #[inline]
    fn bitor_assign(&mut self, other: T) {
        self.0 |= other.into().0;
    }
}

impl<T: Into<Readiness>> ops::BitAnd<T> for Readiness {
    type Output = Readiness;

    #[inline]
    fn bitand(self, other: T) -> Readiness {
        Readiness(self.0 & other.into().0)
    }
}

impl<T: Into<Readiness>> ops::Sub<T> for Readiness {
    type Output = Readiness;

    #[inline]
    fn sub(self, other: T) -> Readiness {
        Readiness(self.0 & !other.into().0)
    }
}
