//! A collection of useful macros for testing futures and tokio based code

/// Assert if a poll is ready
#[macro_export]
macro_rules! assert_ready {
    ($e:expr) => {{
        match $e {
            Ok(::futures::Async::Ready(v)) => v,
            Ok(_) => panic!("not ready"),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
    ($e:expr, $($msg:tt),+) => {{
        match $e {
            Ok(::futures::Async::Ready(v)) => v,
            Ok(_) => {
                let msg = format_args!($($msg),+);
                panic!("not ready; {}", msg)
            }
            Err(e) => {
                let msg = format!($($msg),+);
                panic!("error = {:?}; {}", e, msg)
            }
        }
    }};
}

/// Asset if the poll is not ready
#[macro_export]
macro_rules! assert_not_ready {
    ($e:expr) => {{
        match $e {
            Ok(::futures::Async::NotReady) => {}
            Ok(::futures::Async::Ready(v)) => panic!("ready; value = {:?}", v),
            Err(e) => panic!("error = {:?}", e),
        }
    }};
    ($e:expr, $($msg:tt),+) => {{
        match $e {
            Ok(::futures::Async::NotReady) => {}
            Ok(::futures::Async::Ready(v)) => {
                let msg = format_args!($($msg),+);
                panic!("ready; value = {:?}; {}", v, msg)
            }
            Err(e) => {
                let msg = format_args!($($msg),+);
                panic!("error = {:?}; {}", e, msg)
            }
        }
    }};
}

/// Assert if a poll is ready and check for equality on the value
#[macro_export]
macro_rules! assert_ready_eq {
    ($e:expr, $expect:expr) => {
        match $e {
            Ok(e) => assert_eq!(e, ::futures::Async::Ready($expect)),
            Err(e) => panic!("error = {:?}", e),
        }
    };

    ($e:expr, $expect:expr, $($msg:tt),+) => {
        match $e {
            Ok(e) => assert_eq!(e, ::futures::Async::Ready($expect), $($msg)+),
            Err(e) => {
                let msg = format_args!($($msg),+);
                panic!("error = {:?}; {}", e, msg)
            }
        }
    };
}

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

#[cfg(test)]
mod tests {
    use futures::{future, Async, Future, Poll};

    #[test]
    fn assert_ready() {
        let mut fut = future::ok::<(), ()>(());
        assert_ready!(fut.poll());
        let mut fut = future::ok::<(), ()>(());
        assert_ready!(fut.poll(), "some message");
    }

    #[test]
    #[should_panic]
    fn assert_ready_err() {
        let mut fut = future::err::<(), ()>(());
        assert_ready!(fut.poll());
    }

    #[test]
    fn assert_not_ready() {
        let poll: Poll<(), ()> = Ok(Async::NotReady);
        assert_not_ready!(poll);
        assert_not_ready!(poll, "some message");
    }

    #[test]
    #[should_panic]
    fn assert_not_ready_err() {
        let mut fut = future::err::<(), ()>(());
        assert_not_ready!(fut.poll());
    }

    #[test]
    fn assert_ready_eq() {
        let mut fut = future::ok::<(), ()>(());
        assert_ready_eq!(fut.poll(), ());
    }

}
