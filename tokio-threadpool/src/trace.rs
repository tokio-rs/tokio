#[cfg(not(feature = "trace"))]
macro_rules! t_trace {
    ($msg:expr) => { trace!($msg) };
    ($msg:expr, $($k:ident = $v:expr),* ) => {
        trace!( concat!( $msg, $( stringify!($k), "={:?}; "),* ), $($v),* )
    };
}

#[cfg(feature = "trace")]
macro_rules! t_trace {
    ($msg:expr) => { trace!($msg) };
    ($msg:expr, $($k:ident = $v:expr),* ) => {
        trace!(message = $msg, $($k = $v),* )
    };
}

pub use self::imp::*;

#[cfg(feature = "trace")]
mod imp {
    pub use tokio_trace::{
        field::{debug, display},
        Span,
    };
}

#[cfg(not(feature = "trace"))]
mod imp {
    use std::fmt;

    #[derive(Clone, Debug)]
    pub struct Span {
        _p: (),
    }

    /// A `Value` which serializes as a string using `fmt::Display`.
    #[derive(Clone)]
    pub struct DisplayValue<T: fmt::Display>(T);

    /// A `Value` which serializes as a string using `fmt::Debug`.
    #[derive(Clone)]
    pub struct DebugValue<T: fmt::Debug>(T);

    /// Wraps a type implementing `fmt::Display` as a `Value` that can be
    /// recorded using its `Display` implementation.
    pub fn display<T>(t: T) -> DisplayValue<T>
    where
        T: fmt::Display,
    {
        DisplayValue(t)
    }

    /// Wraps a type implementing `fmt::Debug` as a `Value` that can be
    /// recorded using its `Debug` implementation.
    pub fn debug<T>(t: T) -> DebugValue<T>
    where
        T: fmt::Debug,
    {
        DebugValue(t)
    }

    impl Span {
        #[inline(always)]
        pub fn enter<F: FnOnce() -> T, T>(&mut self, f: F) -> T {
            f()
        }
    }

    impl<T: fmt::Display> fmt::Debug for DisplayValue<T> {
        #[inline]
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Display::fmt(&self.0, f)
        }
    }

    impl<T: fmt::Debug> fmt::Debug for DebugValue<T> {
        #[inline]
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            fmt::Debug::fmt(&self.0, f)
        }
    }

}
