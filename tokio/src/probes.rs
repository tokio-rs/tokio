//! Low-level probes for tools like [perf].
//!
//! [perf]: https://perf.wiki.kernel.org/index.php/Main_Page
use crate::runtime::task::Id;
use probe::probe;

// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!
// IMPORTANT: Never inline probes because this confuses `perf probe`!
// !!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!

/// Begin to poll a task.
#[inline(never)]
#[allow(unused_variables)] // probe!() doesn't use variables on non-supported targets
pub(crate) fn task_poll_begin(id: Id) {
    let id = id.as_u64() as isize;
    probe!(tokio, task_poll_begin, id);
}

/// Finished to poll a task.
#[inline(never)]
#[allow(unused_variables)] // probe!() doesn't use variables on non-supported targets
pub(crate) fn task_poll_end(id: Id, is_ready: bool, panicked: bool) {
    let id = id.as_u64() as isize;
    let is_ready = is_ready as isize;
    let panicked = panicked as isize;
    probe!(tokio, task_poll_end, id, is_ready, panicked);
}

/// Ended a task, so it will never be polled again.
#[inline(never)]
#[allow(unused_variables)] // probe!() doesn't use variables on non-supported targets
pub(crate) fn task_finish(id: Id) {
    let id = id.as_u64() as isize;
    probe!(tokio, task_finish, id);
}

/// Start blocking task.
#[inline(never)]
#[allow(unused_variables)] // probe!() doesn't use variables on non-supported targets
pub(crate) fn task_blocking_begin() {
    probe!(tokio, task_blocking_begin);
}

/// Finish blocking task.
#[inline(never)]
#[allow(unused_variables)] // probe!() doesn't use variables on non-supported targets
pub(crate) fn task_blocking_end() {
    probe!(tokio, task_blocking_end);
}
