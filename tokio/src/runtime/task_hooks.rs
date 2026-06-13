use super::Config;
#[cfg(tokio_unstable)]
use std::any::Any;
use std::marker::PhantomData;
#[cfg(tokio_unstable)]
use std::ptr::NonNull;
use std::sync::Arc;

#[cfg(tokio_unstable)]
pub(crate) type TaskData = Box<dyn Any + Send + Sync + 'static>;

impl TaskHooks {
    pub(crate) fn spawn(&self, meta: &mut TaskMeta<'_>, parent: Option<TaskMetaRef<'_>>) {
        if let Some(f) = self.task_spawn_callback.as_ref() {
            f(meta, parent)
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
    pub(crate) fn has_poll_start_callback(&self) -> bool {
        self.before_poll_callback.is_some()
    }

    #[cfg(tokio_unstable)]
    #[inline]
    pub(crate) fn poll_start_callback(&self, meta: &mut TaskMeta<'_>) {
        if let Some(poll_start) = &self.before_poll_callback {
            (poll_start)(meta);
        }
    }

    #[cfg(tokio_unstable)]
    #[inline]
    pub(crate) fn has_poll_stop_callback(&self) -> bool {
        self.after_poll_callback.is_some()
    }

    #[cfg(tokio_unstable)]
    #[inline]
    pub(crate) fn poll_stop_callback(&self, meta: &mut TaskMeta<'_>) {
        if let Some(poll_stop) = &self.after_poll_callback {
            (poll_stop)(meta);
        }
    }

    #[cfg(tokio_unstable)]
    #[inline]
    pub(crate) fn task_terminate_callback(&self, meta: &mut TaskMeta<'_>) {
        if let Some(task_terminate) = &self.task_terminate_callback {
            (task_terminate)(meta);
        }
    }
}

#[derive(Clone)]
pub(crate) struct TaskHooks {
    pub(crate) task_spawn_callback: Option<TaskSpawnCallback>,
    #[cfg_attr(not(tokio_unstable), allow(dead_code))]
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
    #[cfg(tokio_unstable)]
    pub(crate) user_data: Option<NonNull<Option<TaskData>>>,
    pub(crate) _phantom: PhantomData<&'a ()>,
}

impl<'a> TaskMeta<'a> {
    #[cfg(not(tokio_unstable))]
    pub(crate) fn new(
        id: super::task::Id,
        spawned_at: crate::runtime::task::SpawnLocation,
    ) -> Self {
        Self {
            id,
            spawned_at,
            _phantom: PhantomData,
        }
    }

    /// # Safety
    ///
    /// If `user_data` is present, it must point to live task storage, and this
    /// metadata value must have exclusive access to that storage while it can
    /// expose mutable references to it.
    #[cfg(tokio_unstable)]
    pub(crate) unsafe fn new(
        id: super::task::Id,
        spawned_at: crate::runtime::task::SpawnLocation,
        #[cfg(tokio_unstable)] user_data: Option<NonNull<Option<TaskData>>>,
    ) -> Self {
        Self {
            id,
            spawned_at,
            #[cfg(tokio_unstable)]
            user_data,
            _phantom: PhantomData,
        }
    }

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

    /// Returns a shared reference to this task's user data when the stored type
    /// is `T`.
    #[cfg(tokio_unstable)]
    pub fn data<T: Any>(&self) -> Option<&T> {
        let user_data = self.user_data?;

        // Safety: `TaskMeta` is only constructed while the task allocation is
        // known to be alive. Shared access is allowed for the duration of hook
        // invocation.
        unsafe { user_data.as_ref().as_ref()?.downcast_ref::<T>() }
    }

    /// Returns a mutable reference to this task's user data when the stored type
    /// is `T`.
    #[cfg(tokio_unstable)]
    pub fn data_mut<T: Any>(&mut self) -> Option<&mut T> {
        let mut user_data = self.user_data?;

        // Safety: mutable `TaskMeta` is only handed to hooks for the exact task
        // currently being initialized, polled under the RUNNING task state, or
        // terminated. Tokio does not hold this borrow across polling the future.
        unsafe { user_data.as_mut().as_mut()?.downcast_mut::<T>() }
    }

    /// Replaces this task's user data.
    #[cfg(tokio_unstable)]
    pub fn set_data<T: Any + Send + Sync + 'static>(&mut self, data: T) {
        if let Some(mut user_data) = self.user_data {
            // Safety: see `data_mut`.
            unsafe {
                *user_data.as_mut() = Some(Box::new(data));
            }
        }
    }

    /// Takes this task's user data when the stored type is `T`.
    #[cfg(tokio_unstable)]
    pub fn take_data<T: Any>(&mut self) -> Option<Box<T>> {
        let mut user_data = self.user_data?;

        // Safety: see `data_mut`.
        unsafe {
            let user_data = user_data.as_mut();
            if !user_data.as_ref()?.is::<T>() {
                return None;
            }

            user_data.take()?.downcast::<T>().ok()
        }
    }

    /// Clears this task's user data, returning whether any data was present.
    #[cfg(tokio_unstable)]
    pub fn clear_data(&mut self) -> bool {
        let Some(mut user_data) = self.user_data else {
            return false;
        };

        // Safety: see `data_mut`.
        unsafe { user_data.as_mut().take().is_some() }
    }
}

/// Read-only task metadata supplied to task spawn hooks for parent tasks.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// [unstable]: crate#unstable-features
#[allow(missing_debug_implementations)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct TaskMetaRef<'a> {
    /// The opaque ID of the task.
    pub(crate) id: super::task::Id,
    /// The location where the task was spawned.
    #[cfg_attr(not(tokio_unstable), allow(unreachable_pub, dead_code))]
    pub(crate) spawned_at: crate::runtime::task::SpawnLocation,
    #[cfg(tokio_unstable)]
    pub(crate) user_data: Option<NonNull<Option<TaskData>>>,
    pub(crate) _phantom: PhantomData<&'a ()>,
}

impl<'a> TaskMetaRef<'a> {
    /// # Safety
    ///
    /// If `user_data` is present, it must point to live task storage for the duration
    /// of any references exposed through this metadata value.
    ///
    /// While any references exposed through this metadata value are live, the task
    /// data must not be mutably accessed, replaced, cleared, or dropped.
    #[cfg(tokio_unstable)]
    pub(crate) unsafe fn new(
        id: super::task::Id,
        spawned_at: crate::runtime::task::SpawnLocation,
        #[cfg(tokio_unstable)] user_data: Option<NonNull<Option<TaskData>>>,
    ) -> Self {
        Self {
            id,
            spawned_at,
            #[cfg(tokio_unstable)]
            user_data,
            _phantom: PhantomData,
        }
    }

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

    /// Returns a shared reference to this task's user data when the stored type
    /// is `T`.
    #[cfg(tokio_unstable)]
    pub fn data<T: Any>(&self) -> Option<&T> {
        let user_data = self.user_data?;

        // Safety: `TaskMetaRef` is only constructed while the task allocation is
        // known to be alive. Its constructor requires that the data is not mutated
        // while exposed references are live.
        unsafe { user_data.as_ref().as_ref()?.downcast_ref::<T>() }
    }
}

/// Runs on specific task-related events
pub(crate) type TaskCallback = Arc<dyn Fn(&mut TaskMeta<'_>) + Send + Sync>;

pub(crate) type TaskSpawnCallback =
    Arc<dyn Fn(&mut TaskMeta<'_>, Option<TaskMetaRef<'_>>) + Send + Sync>;
