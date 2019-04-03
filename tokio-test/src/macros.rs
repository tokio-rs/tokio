//! A collection of useful macros for testing futures and tokio based code

/// Assert if a future is ready
#[macro_export]
macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(::futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

/// Asset if the Future is not ready
#[macro_export]
macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(::futures::Async::NotReady) => {}
            Ok(::futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
}

/// Assert if a Future is ready and check for equality on the value
#[macro_export]
macro_rules! assert_ready_eq {
    ($e:expr, $expect:expr) => {
        assert_eq!($e, ::futures::Async::Ready($expect));
    };
}

/// Assert if the deadline has passed
#[macro_export]
macro_rules! assert_elapsed {
    ($e:expr) => {
        assert!($e.unwrap_err().is_elapsed());
    };
}
