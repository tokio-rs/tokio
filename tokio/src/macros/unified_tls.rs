//! Unified thread local storage
//!
//! The number of `thread_local` on some operating systems is limited and may be small.
//! In this case, it is necessary to reduce the usage of `thread_local`.
//!
//! This mod combines multiple `thread_local` into one struct,
//! thus avoiding excessive use of pthread key resources.

use std::cell::RefCell;
use std::error::Error;
use std::fmt;

#[derive(Default)]
pub(crate) struct UnifiedThreadLocal {
    #[cfg(any(feature = "rt", feature = "sync"))]
    pub(crate) thread_current_parker: RefCell<Option<crate::park::thread::ParkThread>>,

    #[cfg(any(feature = "macros"))]
    pub(crate) fast_rand: RefCell<Option<crate::util::rand::FastRand>>,
}

thread_local! {
    pub(crate) static UNIFIED_THREAD_LOCAL: UnifiedThreadLocal =
        UnifiedThreadLocal::default();
}

pub(crate) struct LocalKey<T: 'static> {
    pub(crate) init: fn() -> T,
    pub(crate) with: fn(&UnifiedThreadLocal) -> &RefCell<Option<T>>,
}

macro_rules! unified_thread_local {
    ( static $name:ident with $field:ident : $ty:ty = $init:expr ; ) => {
        static $name: $crate::macros::unified_tls::LocalKey<$ty> =
            $crate::macros::unified_tls::LocalKey {
                init: || $init,
                with: |utls| &utls.$field,
            };
    };
}

impl<T: 'static> LocalKey<T> {
    #[allow(dead_code)]
    pub(crate) fn with<F, R>(&'static self, f: F) -> R
    where
        F: FnOnce(&T) -> R,
    {
        self.try_with(f).expect(
            "cannot access a Thread Local Storage value \
             during or after destruction",
        )
    }

    #[allow(dead_code)]
    pub(crate) fn try_with<F, R>(&'static self, f: F) -> Result<R, AccessError>
    where
        F: FnOnce(&T) -> R,
    {
        UNIFIED_THREAD_LOCAL
            .try_with(|utls| {
                let cell = (self.with)(utls);
                let mut cell = cell.try_borrow_mut().map_err(|_| AccessError)?;
                let cell = cell.get_or_insert_with(self.init);
                Ok(f(cell))
            })
            .map_err(|_| AccessError)?
    }
}

#[non_exhaustive]
pub(crate) struct AccessError;

impl fmt::Debug for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccessError").finish()
    }
}

impl fmt::Display for AccessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt("already destroyed", f)
    }
}

impl Error for AccessError {}
