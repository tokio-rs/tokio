use std::fmt;

/// An opaque ID that uniquely identifies a runtime relative to all other currently
/// running runtimes.
///
/// # Notes
///
/// - Runtime IDs are unique relative to other *currently running* runtimes.
///   When a task completes, the same ID may be used for another task.
/// - Runtime IDs are *not* sequential, and do not indicate the order in which
///   runtimes are started or any other data.
/// - The runtime ID of the currently running task can be obtained from the
///   Handle.
///
/// # Examples
///
/// ```
/// use tokio::runtime::Handle;
///
/// #[tokio::main(flavor = "multi_thread", worker_threads = 4)]
/// async fn main() {
///   println!("Current runtime id: {}", Handle::current().id());
/// }
/// ```
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct Id(u64);

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl Id {
    pub(crate) fn next() -> Self {
        use crate::loom::sync::atomic::{Ordering::Relaxed, StaticAtomicU64};

        static NEXT_ID: StaticAtomicU64 = StaticAtomicU64::new(1);

        Self(NEXT_ID.fetch_add(1, Relaxed))
    }
}
