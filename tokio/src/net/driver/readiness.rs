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
/// let readiness = Readiness::readable() | Readiness::read_closed();
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
    /// let readiness = Readiness::empty();
    ///
    /// assert!(!ready.is_readable());
    /// ```
    pub fn empty() -> Readiness {
        Readiness(0)
    }

    /// Returns a `Readiness` representing readable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::readable();
    ///
    /// assert!(ready.is_readable());
    /// ```
    pub fn readable() -> Readiness {
        Readiness(READABLE)
    }

    /// Returns a `Readiness` representing writable readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::writable();
    ///
    /// assert!(ready.is_writable());
    /// ```
    pub fn writable() -> Readiness {
        Readiness(WRITABLE)
    }

    /// Returns a `Readiness` representing read closed readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::read_closed();
    ///
    /// assert!(ready.is_read_closed());
    /// ```
    pub fn read_closed() -> Readiness {
        Readiness(READ_CLOSED)
    }

    /// Returns a `Readiness` representing write closed readiness.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::write_closed();
    ///
    /// assert!(ready.is_write_closed());
    /// ```
    pub fn write_closed() -> Readiness {
        Readiness(WRITE_CLOSED)
    }

    /// Returns a `Readiness` representing readiness for all operations.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::all();
    ///
    /// assert!(ready.is_readable());
    /// assert!(ready.is_writable());
    /// assert!(ready.is_read_closed());
    /// assert!(ready.is_write_closed());
    /// ```
    pub fn all() -> Readiness {
        Readiness(READABLE | WRITABLE | READ_CLOSED | WRITE_CLOSED)
    }

    /// Returns true if `Readiness` is the empty set
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::empty();
    /// assert!(ready.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        *self == Readiness::empty()
    }

    /// Returns true if the value includes readable readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::readable();
    ///
    /// assert!(ready.is_readable());
    /// ```
    pub fn is_readable(&self) -> bool {
        self.contains(Readiness::readable())
    }

    /// Returns true if the value includes writable readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::writable();
    ///
    /// assert!(ready.is_writable());
    /// ```
    pub fn is_writable(&self) -> bool {
        self.contains(Readiness::writable())
    }

    /// Returns true if the value includes read closed readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::read_closed();
    ///
    /// assert!(ready.is_read_closed());
    /// ```
    pub fn is_read_closed(&self) -> bool {
        self.contains(Readiness::read_closed())
    }

    /// Returns true if the value includes write closed readiness
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let ready = Readiness::write_closed();
    ///
    /// assert!(ready.is_write_closed());
    /// ```
    pub fn is_write_closed(&self) -> bool {
        self.contains(Readiness::write_closed())
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
    /// let readiness = Readiness::readable();
    ///
    /// assert!(readiness.contains(Readiness::readable()));
    /// assert!(!readiness.contains(Readiness::writable()));
    /// ```
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::readable() | Readiness::writable();
    ///
    /// assert!(readiness.contains(Readiness::readable()));
    /// assert!(readiness.contains(Readiness::writable()));
    /// ```
    ///
    /// ```
    /// use tokio::net::driver::Readiness;
    ///
    /// let readiness = Readiness::readable() | Readiness::writable();
    ///
    /// assert!(!Readiness::readable().contains(readiness));
    /// assert!(readiness.contains(readiness));
    /// ```
    pub fn contains<T: Into<Self>>(&self, other: T) -> bool {
        let other = other.into();
        (*self & other) == other
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
    /// let ready = Readiness::readable();
    /// let ready_usize = ready.as_usize();
    /// let ready2 = Readiness::from_usize(ready_usize);
    ///
    /// assert_eq!(ready, ready2);
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
    /// let ready = Readiness::readable();
    /// let ready_usize = ready.as_usize();
    /// let ready2 = Readiness::from_usize(ready_usize);
    ///
    /// assert_eq!(ready, ready2);
    /// ```
    pub fn as_usize(&self) -> usize {
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
