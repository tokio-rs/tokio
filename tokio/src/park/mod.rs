//! Abstraction over blocking and unblocking the current thread.
//!
//! Provides an abstraction over blocking the current thread. This is similar to
//! the park / unpark constructs provided by `std` but made generic. This allows
//! embedding custom functionality to perform when the thread is blocked.
//!
//! A blocked `Park` instance is unblocked by calling `unpark` on its
//! `Unpark` handle.
//!
//! The `ParkThread` struct implements `Park` using `thread::park` to put the
//! thread to sleep. The Tokio reactor also implements park, but uses
//! `mio::Poll` to block the thread instead.
//!
//! The `Park` trait is composable. A timer implementation might decorate a
//! `Park` implementation by checking if any timeouts have elapsed after the
//! inner `Park` implementation unblocks.
//!
//! # Model
//!
//! Conceptually, each `Park` instance has an associated token, which is
//! initially not present:
//!
//! * The `park` method blocks the current thread unless or until the token is
//!   available, at which point it atomically consumes the token.
//! * The `unpark` method atomically makes the token available if it wasn't
//!   already.
//!
//! Some things to note:
//!
//! * If `unpark` is called before `park`, the next call to `park` will
//!   **not** block the thread.
//! * **Spurious** wakeups are permitted, i.e., the `park` method may unblock
//!   even if `unpark` was not called.
//! * `park_timeout` does the same as `park` but allows specifying a maximum
//!   time to block the thread for.

pub(crate) mod thread;
