#[cfg(feature = "triomphe")]
use triomphe::Arc as Inner;
#[cfg(not(feature = "triomphe"))]
use std::sync::Arc as Inner;

/// Faster but more limited version of Arc
#[derive(Debug)]
pub(crate) struct SmallArc<T>(Inner<T>);

impl<T> SmallArc<T> {
    pub (crate) fn new(val: T) -> Self {
        Self(Inner::new(val))
    }
}

// triomphe::Arc has too strict Unpin bounds
impl<T> std::marker::Unpin for SmallArc<T> {}

impl<T> Clone for SmallArc<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> std::ops::Deref for SmallArc<T> {
    type Target = T;
    fn deref(&self) -> &T {
        <Inner<T> as std::ops::Deref>::deref(&self.0)
    }
}
