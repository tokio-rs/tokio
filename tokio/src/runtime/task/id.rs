use crate::runtime::context;

use std::fmt;

/// An opaque ID that uniquely identifies a task relative to all other currently
/// running tasks.
///
/// # Notes
///
/// - Task IDs are unique relative to other *currently running* tasks. When a
///   task completes, the same ID may be used for another task.
/// - Task IDs are *not* sequential, and do not indicate the order in which
///   tasks are spawned, what runtime a task is spawned on, or any other data.
/// - The task ID of the currently running task can be obtained from inside the
///   task via the [`task::try_id()`](crate::task::try_id()) and
///   [`task::id()`](crate::task::id()) functions and from outside the task via
///   the [`JoinHandle::id()`](crate::task::JoinHandle::id()) function.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[cfg_attr(docsrs, doc(cfg(all(feature = "rt", tokio_unstable))))]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub struct Id(u64);

/// Returns the [`Id`] of the currently running task.
///
/// # Panics
///
/// This function panics if called from outside a task. Please note that calls
/// to `block_on` do not have task IDs, so the method will panic if called from
/// within a call to `block_on`. For a version of this function that doesn't
/// panic, see [`task::try_id()`](crate::runtime::task::try_id()).
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [task ID]: crate::task::Id
/// [unstable]: crate#unstable-features
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
#[track_caller]
pub fn id() -> Id {
    context::current_task_id().expect("Can't get a task id when not inside a task")
}

/// Returns the [`Id`] of the currently running task, or `None` if called outside
/// of a task.
///
/// This function is similar to  [`task::id()`](crate::runtime::task::id()), except
/// that it returns `None` rather than panicking if called outside of a task
/// context.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [task ID]: crate::task::Id
/// [unstable]: crate#unstable-features
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
#[track_caller]
pub fn try_id() -> Option<Id> {
    context::current_task_id()
}

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

    pub(crate) fn as_u64(&self) -> u64 {
        self.0
    }
}
