use super::Config;
use std::marker::PhantomData;
#[cfg(feature = "schedule-latency")]
use std::time::Duration;

impl TaskHooks {
    pub(crate) fn spawn(&self, meta: &TaskMeta<'_>) {
        if let Some(f) = self.task_spawn_callback.as_ref() {
            f(meta)
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            task_spawn_callback: config.before_spawn.clone(),
            task_terminate_callback: config.after_termination.clone(),
            #[cfg(tokio_unstable)]
            before_poll_callback: config.before_poll.clone(),
            #[cfg(tokio_unstable)]
            after_poll_callback: config.after_poll.clone(),
        }
    }

    #[cfg(tokio_unstable)]
    #[inline]
    pub(crate) fn poll_start_callback(&self, meta: &TaskMeta<'_>) {
        if let Some(poll_start) = &self.before_poll_callback {
            (poll_start)(meta);
        }
    }

    #[cfg(tokio_unstable)]
    #[inline]
    pub(crate) fn poll_stop_callback(&self, meta: &TaskMeta<'_>) {
        if let Some(poll_stop) = &self.after_poll_callback {
            (poll_stop)(meta);
        }
    }
}

#[derive(Clone)]
pub(crate) struct TaskHooks {
    pub(crate) task_spawn_callback: Option<TaskCallback>,
    pub(crate) task_terminate_callback: Option<TaskCallback>,
    #[cfg(tokio_unstable)]
    pub(crate) before_poll_callback: Option<TaskCallback>,
    #[cfg(tokio_unstable)]
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
    /// The location where the task was spawned.
    #[cfg_attr(not(tokio_unstable), allow(unreachable_pub, dead_code))]
    pub(crate) spawned_at: crate::runtime::task::SpawnLocation,
    /// The latency between scheduling the task and starting its current poll.
    #[cfg(feature = "schedule-latency")]
    pub(crate) schedule_latency: Option<Duration>,
    pub(crate) _phantom: PhantomData<&'a ()>,
}

impl<'a> TaskMeta<'a> {
    /// Return the opaque ID of the task.
    #[cfg_attr(not(tokio_unstable), allow(unreachable_pub, dead_code))]
    pub fn id(&self) -> super::task::Id {
        self.id
    }

    /// Return the source code location where the task was spawned.
    #[cfg(tokio_unstable)]
    pub fn spawned_at(&self) -> &'static std::panic::Location<'static> {
        self.spawned_at.0
    }

    /// Returns the latency between scheduling the task and starting its
    /// current poll.
    ///
    /// Task schedule latency is measured from the task's most recent transition
    /// to the scheduled state until immediately before the
    /// [`on_before_task_poll`] callback. Waking a task that is already scheduled
    /// does not reset the measurement.
    ///
    /// This returns `Some` from [`on_before_task_poll`] and
    /// [`on_after_task_poll`] callbacks when tracking is enabled with
    /// [`track_task_schedule_latency`] or
    /// [`enable_metrics_schedule_latency_histogram`]. Both callbacks receive
    /// the same value, so time spent polling the task is not included. It
    /// returns `None` when tracking is disabled and from task spawn and
    /// termination callbacks.
    ///
    /// [`track_task_schedule_latency`]: crate::runtime::Builder::track_task_schedule_latency
    /// [`enable_metrics_schedule_latency_histogram`]: crate::runtime::Builder::enable_metrics_schedule_latency_histogram
    /// [`on_before_task_poll`]: crate::runtime::Builder::on_before_task_poll
    /// [`on_after_task_poll`]: crate::runtime::Builder::on_after_task_poll
    #[cfg(feature = "schedule-latency")]
    #[cfg_attr(docsrs, doc(cfg(feature = "schedule-latency")))]
    pub fn schedule_latency(&self) -> Option<Duration> {
        self.schedule_latency
    }
}

/// Runs on specific task-related events
pub(crate) type TaskCallback = std::sync::Arc<dyn Fn(&TaskMeta<'_>) + Send + Sync>;
