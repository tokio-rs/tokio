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
    pub fn empty() -> Readiness {
        Readiness(0)
    }

    pub fn readable() -> Readiness {
        Readiness(READABLE)
    }

    pub fn writable() -> Readiness {
        Readiness(WRITABLE)
    }

    pub fn read_closed() -> Readiness {
        Readiness(READ_CLOSED)
    }

    pub fn write_closed() -> Readiness {
        Readiness(WRITE_CLOSED)
    }

    pub fn all() -> Readiness {
        Readiness(READABLE | WRITABLE | READ_CLOSED | WRITE_CLOSED)
    }

    pub fn is_empty(&self) -> bool {
        *self == Readiness::empty()
    }

    pub fn is_readable(&self) -> bool {
        self.contains(Readiness::readable())
    }

    pub fn is_writable(&self) -> bool {
        self.contains(Readiness::writable())
    }

    pub fn is_read_closed(&self) -> bool {
        self.contains(Readiness::read_closed())
    }

    pub fn is_write_closed(&self) -> bool {
        self.contains(Readiness::write_closed())
    }

    pub fn contains<T: Into<Self>>(&self, other: T) -> bool {
        let other = other.into();
        (*self & other) == other
    }

    pub fn from_usize(val: usize) -> Readiness {
        Readiness(val)
    }

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

impl From<usize> for Readiness {
    fn from(x: usize) -> Self {
        Readiness(x)
    }
}
