//! Abstraction over blocking and unblocking the current thread.
//!
//! Provides an abstraction over blocking the current thread. This is similar to
//! the park / unpark constructs provided by [`std`] but made generic. This
//! allows embedding custom functionality to perform when the thread is blocked.
//!
//! A blocked [`Park`][p] instance is unblocked by calling [`unpark`] on its
//! [`Unpark`][up] handle.
//!
//! The [`ParkThread`] struct implements [`Park`][p] using
//! [`thread::park`][`std`] to put the thread to sleep. The Tokio reactor also
//! implements park, but uses [`mio::Poll`][mio] to block the thread instead.
//!
//! The [`Park`][p] trait is composable. A timer implementation might decorate a
//! [`Park`][p] implementation by checking if any timeouts have elapsed after
//! the inner [`Park`][p] implementation unblocks.
//!
//! # Model
//!
//! Conceptually, each [`Park`][p] instance has an associated token, which is
//! initially not present:
//!
//! * The [`park`] method blocks the current thread unless or until the token
//!   is available, at which point it atomically consumes the token.
//! * The [`unpark`] method atomically makes the token available if it wasn't
//!   already.
//!
//! Some things to note:
//!
//! * If [`unpark`] is called before [`park`], the next call to [`park`] will
//!   **not** block the thread.
//! * **Spurious** wakeups are permitted, i.e., the [`park`] method may unblock
//!   even if [`unpark`] was not called.
//! * [`park_timeout`] does the same as [`park`] but allows specifying a maximum
//!   time to block the thread for.
//!
//! [`std`]: https://doc.rust-lang.org/std/thread/fn.park.html
//! [`thread::park`]: https://doc.rust-lang.org/std/thread/fn.park.html
//! [`ParkThread`]: struct.ParkThread.html
//! [p]: trait.Park.html
//! [`park`]: trait.Park.html#tymethod.park
//! [`park_timeout`]: trait.Park.html#tymethod.park_timeout
//! [`unpark`]: trait.Unpark.html#tymethod.unpark
//! [up]: trait.Unpark.html
//! [mio]: https://docs.rs/mio/0.6/mio/struct.Poll.html

mod park;
pub use self::park::{Park, Unpark};

mod thread;
pub use self::thread::{ParkError, ParkThread, UnparkThread};
