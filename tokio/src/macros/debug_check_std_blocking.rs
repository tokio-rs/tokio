//! Debug assertions that from_std_assume_nonblocking is not used on blocking sockets.
//!
//! These are displayed warnings in debug mode, panics in test mode
//! (so that nothing slips through in the tokio test suite), and no-ops in release mode.

#[cfg(all(debug_assertions, not(test)))]
/// Debug assertions that from_std_assume_nonblocking is not used on blocking sockets.
///
/// These are displayed warnings in debug mode, panics in test mode
/// (so that nothing slips through in the tokio test suite), and no-ops in release mode.
macro_rules! debug_check_non_blocking {
    ($std_socket: expr, $method: expr, $fallback_method: expr) => {{
        // Make sure the provided item is in non-blocking mode, otherwise warn.
        static HAS_WARNED_BLOCKING: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        match socket2::SockRef::from(&$std_socket).nonblocking() {
            Ok(true) => {}
            Ok(false) => {
                if !HAS_WARNED_BLOCKING.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    println!(concat!(
                        "WARNING: `",
                        $method,
                        "` was called on a socket that is \
                            not in non-blocking mode. This is unexpected, and may cause the \
                            thread to block indefinitely. Use `",
                        $fallback_method,
                        "` instead."
                    ));
                }
            }
            Err(io_error) => {
                if !HAS_WARNED_BLOCKING.swap(true, std::sync::atomic::Ordering::Relaxed) {
                    println!(
                        concat!(
                            "WARNING: `",
                            $method,
                            "` was called on a socket which we \
                                could not determine whether was in non-blocking mode: {}"
                        ),
                        io_error
                    );
                }
            }
        }
    }};
}

#[cfg(test)]
/// Debug assertions that from_std_assume_nonblocking is not used on blocking sockets.
///
/// These are displayed warnings in debug mode, panics in test mode
/// (so that nothing slips through in the tokio test suite), and no-ops in release mode.
macro_rules! debug_check_non_blocking {
    ($std_socket: expr, $method: expr, $fallback_method: expr) => {{
        match socket2::SockRef::from(&$std_socket).nonblocking() {
            Ok(true) => {}
            Ok(false) => {
                panic!(concat!(
                    $method,
                    "` was called on a socket that is \
                        not in non-blocking mode. This is unexpected, and may cause the \
                        thread to block indefinitely. Use `",
                    $fallback_method,
                    "` instead."
                ))
            }
            Err(io_error) => {
                panic!(
                    concat!(
                        $method,
                        "` was called on a socket which we \
                            could not determine whether was in non-blocking mode: {}"
                    ),
                    io_error
                );
            }
        }
    }};
}

#[cfg(not(debug_assertions))]
/// Debug assertions that from_std_assume_nonblocking is not used on blocking sockets.
///
/// These are displayed warnings in debug mode, panics in test mode
/// (so that nothing slips through in the tokio test suite), and no-ops in release mode.
macro_rules! debug_check_non_blocking {
    ($($tts:tt)+) => {};
}
