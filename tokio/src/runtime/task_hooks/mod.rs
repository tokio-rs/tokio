use super::task;
use crate::loom::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ptr::NonNull;
use std::sync::Arc;

/// A factory which produces new [`TaskHookHarness`] objects for tasks which either have been
/// spawned in "detached mode" via [`tokio::task::spawn_with_hooks`], or which were spawned from outside the runtime or
/// from another context where no [`TaskHookHarness`] was present.
pub trait TaskHookHarnessFactory {
    /// Create a new [`TaskHookHarness`] object which the runtime will attach to a given task.
    fn on_top_level_spawn(
        &self,
        ctx: &mut OnTopLevelTaskSpawnContext<'_>,
    ) -> Option<Box<dyn TaskHookHarness + Send + Sync + 'static>>;
}

/// Trait for user-provided "harness" objects which are attached to tasks and provide hook
/// implementations.
#[allow(unused_variables)]
pub trait TaskHookHarness {
    /// Pre-poll task hook which runs arbitrary user logic.
    fn before_poll(&mut self, ctx: &mut BeforeTaskPollContext<'_>) {}

    /// Post-poll task hook which runs arbitrary user logic.
    fn after_poll(&mut self, ctx: &mut AfterTaskPollContext<'_>) {}

    /// Task hook which runs when this task spawns a child, unless that child is explicitly spawned
    /// detached from the parent.
    ///
    /// This hook creates a harness for the child, or detaches the child from any instrumentation.
    fn on_child_spawn(
        &mut self,
        ctx: &mut OnChildTaskSpawnContext<'_>,
    ) -> Option<Box<dyn TaskHookHarness + Send + Sync + 'static>> {
        None
    }

    /// Task hook which runs on task termination.
    fn on_task_terminate(&mut self, ctx: &mut OnTaskTerminateContext<'_>) {}
}

pub(crate) type OptionalTaskHooksFactory =
    Option<Arc<dyn TaskHookHarnessFactory + Send + Sync + 'static>>;
pub(crate) type OptionalTaskHooks = Option<Box<dyn TaskHookHarness + Send + Sync + 'static>>;

pub(crate) type OptionalTaskHooksWeak =
    UnsafeCell<Option<NonNull<dyn TaskHookHarness + Send + Sync + 'static>>>;

pub(crate) type OptionalTaskHooksMut<'a> =
    Option<&'a mut (dyn TaskHookHarness + Send + Sync + 'static)>;
pub(crate) type OptionalTaskHooksFactoryRef<'a> =
    Option<&'a (dyn TaskHookHarnessFactory + Send + Sync + 'static)>;

#[allow(missing_debug_implementations, missing_docs)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct OnTopLevelTaskSpawnContext<'a> {
    pub(crate) id: task::Id,
    pub(crate) _phantom: PhantomData<&'a ()>,
}

impl<'a> OnTopLevelTaskSpawnContext<'a> {
    /// Returns the ID of the task.
    pub fn id(&self) -> task::Id {
        self.id
    }
}

#[allow(missing_debug_implementations, missing_docs)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct OnChildTaskSpawnContext<'a> {
    pub(crate) id: task::Id,
    pub(crate) _phantom: PhantomData<&'a ()>,
}

impl<'a> OnChildTaskSpawnContext<'a> {
    /// Returns the ID of the task.
    pub fn id(&self) -> task::Id {
        self.id
    }
}

#[allow(missing_debug_implementations, missing_docs)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct OnTaskTerminateContext<'a> {
    pub(crate) _phantom: PhantomData<&'a ()>,
}

#[allow(missing_debug_implementations, missing_docs)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct BeforeTaskPollContext<'a> {
    pub(crate) _phantom: PhantomData<&'a ()>,
}

#[allow(missing_debug_implementations, missing_docs)]
#[cfg_attr(not(tokio_unstable), allow(unreachable_pub))]
pub struct AfterTaskPollContext<'a> {
    pub(crate) _phantom: PhantomData<&'a ()>,
}
