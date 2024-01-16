use std::{
    ops::{Deref, DerefMut},
    sync::{self, LockResult, PoisonError},
};

/// Adapter for std::RwLock that adds `downgrade` method.
#[derive(Debug)]
pub(crate) struct RwLock<T>(sync::RwLock<T>);

#[derive(Debug)]
pub(crate) struct RwLockReadGuard<'a, T>(sync::RwLockReadGuard<'a, T>);

#[derive(Debug)]
pub(crate) struct RwLockWriteGuard<'a, T>(sync::RwLockWriteGuard<'a, T>);

#[allow(dead_code)]
impl<T> RwLock<T> {
    #[inline]
    pub(crate) fn new(t: T) -> RwLock<T> {
        RwLock(sync::RwLock::new(t))
    }

    #[inline]
    pub(crate) fn read(&self) -> LockResult<RwLockReadGuard<'_, T>> {
        match self.0.read() {
            Ok(inner) => Ok(RwLockReadGuard(inner)),
            Err(err) => Err(PoisonError::new(RwLockReadGuard(err.into_inner()))),
        }
    }

    #[inline]
    pub(crate) fn write(&self) -> LockResult<RwLockWriteGuard<'_, T>> {
        match self.0.write() {
            Ok(inner) => Ok(RwLockWriteGuard(inner)),
            Err(err) => Err(PoisonError::new(RwLockWriteGuard(err.into_inner()))),
        }
    }
}

impl<'a, T> Deref for RwLockReadGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.deref()
    }
}

impl<'a, T> RwLockWriteGuard<'a, T> {
    pub(crate) fn downgrade(s: Self, rwlock: &'a RwLock<T>) -> LockResult<RwLockReadGuard<'a, T>> {
        // Std rwlock does not support downgrading.
        drop(s);
        rwlock.read()
    }
}

impl<'a, T> Deref for RwLockWriteGuard<'a, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.0.deref()
    }
}

impl<'a, T> DerefMut for RwLockWriteGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.0.deref_mut()
    }
}
