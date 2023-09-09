cfg_rt! {
use crate::runtime::task::Notified;
use crate::runtime::task::Task;

pub(crate) fn release(task: &Task) -> Option<Task> {
    use crate::runtime::context;

    match context::with_current(|handle| handle.release(task)) {
        Ok(join_handle) => join_handle,
        Err(e) => panic!("{}", e),
    }
}

pub(crate) fn schedule(task: Notified) {
    use crate::runtime::context;

    match context::with_current(|handle| handle.schedule(task)) {
        Ok(join_handle) => join_handle,
        Err(e) => panic!("{}", e),
    }
}

pub(crate) fn yield_now(task: Notified) {
    use crate::runtime::context;

    match context::with_current(|handle| handle.yield_now(task)) {
        Ok(join_handle) => join_handle,
        Err(e) => panic!("{}", e),
    }
}
}
