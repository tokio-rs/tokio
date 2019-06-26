//! A collection of useful macros for testing futures and tokio based code

/// Assert a `Poll` is ready, returning the value.
#[macro_export]
macro_rules! assert_ready {
    ($e:expr) => {{
        use core::task::Poll::*;
        match $e {
            Ready(v) => v,
            Pending => panic!("pending"),
        }
    }};
    ($e:expr, $($msg:tt),+) => {{
        use core::task::Poll::*;
        match $e {
            Ready(v) => v,
            Pending => {
                let msg = format_args!($($msg),+);
                panic!("pending; {}", msg)
            }
        }
    }};
}

/// Assert a `Poll<Result<...>>` is ready and `Ok`, returning the value.
#[macro_export]
macro_rules! assert_ready_ok {
    ($e:expr) => {{
        use tokio_test::{assert_ready, assert_ok};
        let val = assert_ready!($e);
        assert_ok!(val)
    }};
    ($e:expr, $($msg:tt),+) => {{
        use tokio_test::{assert_ready, assert_ok};
        let val = assert_ready!($e, $($msg),*);
        assert_ok!(val, $($msg),*)
    }};
}

/// Assert a `Poll<Result<...>>` is ready and `Err`, returning the error.
#[macro_export]
macro_rules! assert_ready_err {
    ($e:expr) => {{
        use tokio_test::{assert_ready, assert_err};
        let val = assert_ready!($e);
        assert_err!(val)
    }};
    ($e:expr, $($msg:tt),+) => {{
        use tokio_test::{assert_ready, assert_err};
        let val = assert_ready!($e, $($msg),*);
        assert_err!(val, $($msg),*)
    }};
}

/// Asset a `Poll` is pending.
#[macro_export]
macro_rules! assert_pending {
    ($e:expr) => {{
        use core::task::Poll::*;
        match $e {
            Pending => {}
            Ready(v) => panic!("ready; value = {:?}", v),
        }
    }};
    ($e:expr, $($msg:tt),+) => {{
        use core::task::Poll::*;
        match $e {
            Pending => {}
            Ready(v) => {
                let msg = format_args!($($msg),+);
                panic!("ready; value = {:?}; {}", v, msg)
            }
        }
    }};
}

/*
/// Assert if a poll is ready and check for equality on the value
#[macro_export]
macro_rules! assert_ready_eq {
    ($e:expr, $expect:expr) => {
        use $crate::codegen::futures::Async::Ready;
        match $e {
            Ok(e) => assert_eq!(e, Ready($expect)),
            Err(e) => panic!("error = {:?}", e),
        }
    };

    ($e:expr, $expect:expr, $($msg:tt),+) => {
        use $crate::codegen::futures::Async::Ready;
        match $e {
            Ok(e) => assert_eq!(e, Ready($expect), $($msg)+),
            Err(e) => {
                let msg = format_args!($($msg),+);
                panic!("error = {:?}; {}", e, msg)
            }
        }
    };
}
*/

/*
/// Assert if the deadline has passed
#[macro_export]
macro_rules! assert_elapsed {
    ($e:expr) => {
        assert!($e.unwrap_err().is_elapsed());
    };

    ($e:expr, $($msg:expr),+) => {
        assert!($e.unwrap_err().is_elapsed(), $msg);
    };
}
*/
