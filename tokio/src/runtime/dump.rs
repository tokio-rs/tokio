//! Snapshots of runtime state.
//!
//! See [Handle::dump][crate::runtime::Handle::dump].

use crate::task::Id;
use std::fmt;

/// A snapshot of a runtime's state.
///
/// See [Handle::dump][crate::runtime::Handle::dump].
#[derive(Debug)]
pub struct Dump {
    tasks: Tasks,
}

/// Snapshots of tasks.
///
/// See [Handle::dump][crate::runtime::Handle::dump].
#[derive(Debug)]
pub struct Tasks {
    tasks: Vec<Task>,
}

/// A snapshot of a task.
///
/// See [Handle::dump][crate::runtime::Handle::dump].
#[derive(Debug)]
pub struct Task {
    id: Id,
    trace: Trace,
}

/// An execution trace of a task's last poll.
///
/// See [Handle::dump][crate::runtime::Handle::dump].
#[derive(Debug)]
pub struct Trace {
    inner: super::task::trace::Trace,
}

impl Dump {
    pub(crate) fn new(tasks: Vec<Task>) -> Self {
        Self {
            tasks: Tasks { tasks },
        }
    }

    /// Tasks in this snapshot.
    pub fn tasks(&self) -> &Tasks {
        &self.tasks
    }
}

impl Tasks {
    /// Iterate over tasks.
    pub fn iter(&self) -> impl Iterator<Item = &Task> {
        self.tasks.iter()
    }
}

impl Task {
    pub(crate) fn new(id: Id, trace: super::task::trace::Trace) -> Self {
        Self {
            id,
            trace: Trace { inner: trace },
        }
    }

    /// Returns a [task ID] that uniquely identifies this task relative to other
    /// tasks spawned at the time of the dump.
    ///
    /// **Note**: This is an [unstable API][unstable]. The public API of this type
    /// may break in 1.x releases. See [the documentation on unstable
    /// features][unstable] for details.
    ///
    /// [task ID]: crate::task::Id
    /// [unstable]: crate#unstable-features
    #[cfg(tokio_unstable)]
    #[cfg_attr(docsrs, doc(cfg(tokio_unstable)))]
    pub fn id(&self) -> Id {
        self.id
    }

    /// A trace of this task's state.
    pub fn trace(&self) -> &Trace {
        &self.trace
    }
}

impl fmt::Display for Trace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}
