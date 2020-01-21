//! This module abstracts over `loom` and `std::sync` (or parking_lot)
//! depending on features or whether we are running tests or not.

#[cfg(not(all(test, loom)))]
mod std;
#[cfg(not(all(test, loom)))]
pub(crate) use self::std::*;

#[cfg(all(test, loom))]
mod mocked;
#[cfg(all(test, loom))]
pub(crate) use self::mocked::*;

pub(crate) mod sync {
    mod expect_poison;
    pub(crate) use expect_poison::ExpectPoison;

    #[cfg(not(all(test, loom)))]
    pub(crate) use super::std::sync::*;

    #[cfg(all(test, loom))]
    pub(crate) use loom::sync::*;
}
