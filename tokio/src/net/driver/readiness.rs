use std::ops;

const READABLE: usize = 0b0_01;
const WRITABLE: usize = 0b0_10;
const READ_CLOSED: usize = 0b0_0100;
const WRITE_CLOSED: usize = 0b0_1000;

/// TODO
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Readiness(usize);

impl Readiness {
    pub(crate) fn empty() -> Readiness {
        Readiness(0)
    }

    pub(crate) fn readable() -> Readiness {
        Readiness(READABLE)
    }

    pub(crate) fn writable() -> Readiness {
        Readiness(WRITABLE)
    }

    pub(crate) fn read_closed() -> Readiness {
        Readiness(READ_CLOSED)
    }

    pub(crate) fn write_closed() -> Readiness {
        Readiness(WRITE_CLOSED)
    }

    pub(crate) fn all() -> Readiness {
        Readiness(READABLE | WRITABLE | READ_CLOSED | WRITE_CLOSED)
    }

    pub(crate) fn is_empty(&self) -> bool {
        *self == Readiness::empty()
    }

    pub(crate) fn is_readable(&self) -> bool {
        self.contains(Readiness::readable())
    }

    pub(crate) fn is_writable(&self) -> bool {
        self.contains(Readiness::writable())
    }

    pub(crate) fn is_read_closed(&self) -> bool {
        self.contains(Readiness::read_closed())
    }

    pub(crate) fn is_write_closed(&self) -> bool {
        self.contains(Readiness::write_closed())
    }

    pub(crate) fn contains<T: Into<Self>>(&self, other: T) -> bool {
        let other = other.into();
        (*self & other) == other
    }

    pub(crate) fn from_usize(val: usize) -> Readiness {
        Readiness(val)
    }

    pub(crate) fn as_usize(&self) -> usize {
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
