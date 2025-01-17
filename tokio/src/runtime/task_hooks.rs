use std::marker::PhantomData;

use super::Config;

impl TaskHooks {
    pub(crate) fn spawn(&self, meta: &TaskMeta<'_>) {
        if let Some(f) = self.task_spawn_callback.as_ref() {
            f(meta)
        }
    }

    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            task_spawn_callback: config.before_spawn.clone(),
            task_terminate_callback: config.after_termination.clone(),
            before_poll_callback: config.before_poll.clone(),
            after_poll_callback: config.after_poll.clone(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct TaskHooks {
    pub(crate) task_spawn_callback: Option<TaskCallback>,
    pub(crate) task_terminate_callback: Option<TaskCallback>,
    pub(crate) before_poll_callback: Option<TaskCallback>,
    pub(crate) after_poll_callback: Option<TaskCallback>,
}

/// Task metadata supplied to user-provided hooks for task events.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[allow(missing_debug_implementations)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct TaskMeta<'a> {
    /// The opaque ID of the task.
    pub(crate) id: super::task::Id,
    pub(crate) _phantom: PhantomData<&'a ()>,
}

impl<'a> TaskMeta<'a> {
    /// Return the opaque ID of the task.
    #[cfg_attr(not(tokio_unstable), allow(unreachable_pub, dead_code))]
    pub fn id(&self) -> super::task::Id {
        self.id
    }
}

/// Runs on specific task-related events
pub(crate) type TaskCallback = std::sync::Arc<dyn Fn(&TaskMeta<'_>) + Send + Sync>;
