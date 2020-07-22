//! A collection of useful macros for testing futures and tokio based code

/// Asserts a `Poll` is ready, returning the value.
///
/// This will invoke `panic!` if the provided `Poll` does not evaluate to `Poll::Ready` at
/// runtime.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use futures_util::future;
/// use tokio_test::{assert_ready, task};
///
/// let mut fut = task::spawn(future::ready(()));
/// assert_ready!(fut.poll());
/// ```
#[macro_export]
macro_rules! assert_ready {
    ($e:expr) => {{
        use core::task::Poll::*;
        match $e {
            Ready(v) => v,
            Pending => panic!("pending"),
        }
    }};
    ($e:expr, $($msg:tt)+) => {{
        use core::task::Poll::*;
        match $e {
            Ready(v) => v,
            Pending => {
                panic!("pending; {}", format_args!($($msg)+))
            }
        }
    }};
}

/// Asserts a `Poll<Result<...>>` is ready and `Ok`, returning the value.
///
/// This will invoke `panic!` if the provided `Poll` does not evaluate to `Poll::Ready(Ok(..))` at
/// runtime.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use futures_util::future;
/// use tokio_test::{assert_ready_ok, task};
///
/// let mut fut = task::spawn(future::ok::<_, ()>(()));
/// assert_ready_ok!(fut.poll());
/// ```
#[macro_export]
macro_rules! assert_ready_ok {
    ($e:expr) => {{
        use tokio_test::{assert_ready, assert_ok};
        let val = assert_ready!($e);
        assert_ok!(val)
    }};
    ($e:expr, $($msg:tt)+) => {{
        use tokio_test::{assert_ready, assert_ok};
        let val = assert_ready!($e, $($msg)*);
        assert_ok!(val, $($msg)*)
    }};
}

/// Asserts a `Poll<Result<...>>` is ready and `Err`, returning the error.
///
/// This will invoke `panic!` if the provided `Poll` does not evaluate to `Poll::Ready(Err(..))` at
/// runtime.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use futures_util::future;
/// use tokio_test::{assert_ready_err, task};
///
/// let mut fut = task::spawn(future::err::<(), _>(()));
/// assert_ready_err!(fut.poll());
/// ```
#[macro_export]
macro_rules! assert_ready_err {
    ($e:expr) => {{
        use tokio_test::{assert_ready, assert_err};
        let val = assert_ready!($e);
        assert_err!(val)
    }};
    ($e:expr, $($msg:tt)+) => {{
        use tokio_test::{assert_ready, assert_err};
        let val = assert_ready!($e, $($msg)*);
        assert_err!(val, $($msg)*)
    }};
}

/// Asserts a `Poll` is pending.
///
/// This will invoke `panic!` if the provided `Poll` does not evaluate to `Poll::Pending` at
/// runtime.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use futures_util::future;
/// use tokio_test::{assert_pending, task};
///
/// let mut fut = task::spawn(future::pending::<()>());
/// assert_pending!(fut.poll());
/// ```
#[macro_export]
macro_rules! assert_pending {
    ($e:expr) => {{
        use core::task::Poll::*;
        match $e {
            Pending => {}
            Ready(v) => panic!("ready; value = {:?}", v),
        }
    }};
    ($e:expr, $($msg:tt)+) => {{
        use core::task::Poll::*;
        match $e {
            Pending => {}
            Ready(v) => {
                panic!("ready; value = {:?}; {}", v, format_args!($($msg)+))
            }
        }
    }};
}

/// Asserts if a poll is ready and check for equality on the value
///
/// This will invoke `panic!` if the provided `Poll` does not evaluate to `Poll::Ready` at
/// runtime and the value produced does not partially equal the expected value.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use futures_util::future;
/// use tokio_test::{assert_ready_eq, task};
///
/// let mut fut = task::spawn(future::ready(42));
/// assert_ready_eq!(fut.poll(), 42);
/// ```
#[macro_export]
macro_rules! assert_ready_eq {
    ($e:expr, $expect:expr) => {
        let val = $crate::assert_ready!($e);
        assert_eq!(val, $expect)
    };

    ($e:expr, $expect:expr, $($msg:tt)+) => {
        let val = $crate::assert_ready!($e, $($msg)*);
        assert_eq!(val, $expect, $($msg)*)
    };
}

/// Asserts that the expression evaluates to `Ok` and returns the value.
///
/// This will invoke the `panic!` macro if the provided expression does not evaluate to `Ok` at
/// runtime.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use tokio_test::assert_ok;
///
/// let n: u32 = assert_ok!("123".parse());
///
/// let s = "123";
/// let n: u32 = assert_ok!(s.parse(), "testing parsing {:?} as a u32", s);
/// ```
#[macro_export]
macro_rules! assert_ok {
    ($e:expr) => {
        assert_ok!($e,)
    };
    ($e:expr,) => {{
        use std::result::Result::*;
        match $e {
            Ok(v) => v,
            Err(e) => panic!("assertion failed: Err({:?})", e),
        }
    }};
    ($e:expr, $($arg:tt)+) => {{
        use std::result::Result::*;
        match $e {
            Ok(v) => v,
            Err(e) => panic!("assertion failed: Err({:?}): {}", e, format_args!($($arg)+)),
        }
    }};
}

/// Asserts that the expression evaluates to `Err` and returns the error.
///
/// This will invoke the `panic!` macro if the provided expression does not evaluate to `Err` at
/// runtime.
///
/// # Custom Messages
///
/// This macro has a second form, where a custom panic message can be provided with or without
/// arguments for formatting.
///
/// # Examples
///
/// ```
/// use tokio_test::assert_err;
/// use std::str::FromStr;
///
///
/// let err = assert_err!(u32::from_str("fail"));
///
/// let msg = "fail";
/// let err = assert_err!(u32::from_str(msg), "testing parsing {:?} as u32", msg);
/// ```
#[macro_export]
macro_rules! assert_err {
    ($e:expr) => {
        assert_err!($e,);
    };
    ($e:expr,) => {{
        use std::result::Result::*;
        match $e {
            Ok(v) => panic!("assertion failed: Ok({:?})", v),
            Err(e) => e,
        }
    }};
    ($e:expr, $($arg:tt)+) => {{
        use std::result::Result::*;
        match $e {
            Ok(v) => panic!("assertion failed: Ok({:?}): {}", v, format_args!($($arg)+)),
            Err(e) => e,
        }
    }};
}
