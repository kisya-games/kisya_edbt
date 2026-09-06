//! [`BehaviourNode's`][BehaviourNode] task components and plugins for bevy ecs.

use std::marker::PhantomData;

use bevy::{
    ecs::{component::ComponentId, event::EntityComponentsTrigger},
    prelude::*,
    reflect::GetTypeRegistration,
};

use crate::core::{
    node::{BehaviourNode, TreeNodeId},
    runner::{BehaviourNewRunSchedule, BehaviourUpdateCycleSchedule, BehaviourUpdateCycleSystems},
    spawn::DynamicNodeSpawner,
    tree::ReflectDynamicNode,
};

/// Entity event for a new [`TaskWorker`] spawn.
///
/// Observers can be gated by the source node type exactly like lifecycle events
/// are gated by a component: `On<TaskAdded, TaskWorker<WhileNode>>`
/// only runs when the source task belongs to a `WhileNode`.
///
/// *NOTE*: Internally, this is just [`Insert`]. It is not [`Add`] on purpose:
/// `Insert` observers fire after `on_insert` hooks, so the
/// `TaskOf`/`TaskChildOf` relationship updates are already queued before a
/// setup observer can queue a finish status. That keeps command order correct
/// even when the whole task bundle is inserted in one go and the task finishes
/// instantly.
pub type TaskAdded = Insert;

/// Plugin for general task logic.
pub struct TaskPlugin;

impl Plugin for TaskPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().register_disabling_component::<DisabledTask>();

        app.add_systems(BehaviourNewRunSchedule, new_run_system);
        app.add_observer(on_task_status_inserted_hook);
    }
}

/// Plugin for concrete typed task logic.
pub struct TaskWorkerPlugin<N: BehaviourNode> {
    _phantom: PhantomData<N>,
}

impl<N: BehaviourNode> TaskWorkerPlugin<N> {
    /// Create a new plugin for some type of node.
    pub fn new() -> Self { Self { _phantom: PhantomData } }
}

impl<N> Plugin for TaskWorkerPlugin<N>
where
    N: BehaviourNode + GetTypeRegistration + TypePath,
    N::Task: FromReflect + GetTypeRegistration + TypePath,
{
    fn build(&self, app: &mut App) {
        app.add_systems(
            BehaviourUpdateCycleSchedule,
            task_workers_system::<N>
                .run_if(any_with_component::<TaskWorker<N>>)
                .in_set(BehaviourUpdateCycleSystems::PostUpdate),
        );
        app.register_type::<TaskWorker<N>>()
            .register_type_data::<N, DynamicNodeSpawner>()
            .register_type_data::<N, ReflectDynamicNode>();
    }
}

/// Actual status of a [`TaskWorker`]'s task.
///
/// Inserting a finish status is the only way to finish a task; but it will
/// trigger [`TaskFinished`].
#[derive(
    Component, Clone, Copy, PartialEq, Eq, PartialOrd, Hash, Debug, Reflect, Default, strum::Display,
)]
#[component(immutable)]
pub enum TaskStatus {
    /// Task is currently in work.
    #[default]
    Running,
    /// Task is finished and there are no errors.
    Success,
    /// Task is finished but something went wrong.
    Failure,
}

impl TaskStatus {
    /// Checks if status is finished: either [`TaskStatus::Success`] or
    /// [`TaskStatus::Failure`].
    pub fn is_finished(&self) -> bool { matches!(self, TaskStatus::Success | TaskStatus::Failure) }
}

/// Non-generic per-task info.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[component(immutable)]
pub struct TaskInfo {
    /// Node (and its tree) this task was spawned from.
    pub(crate) source: TreeNodeId,
    /// [`ComponentId`] of this task's `TaskWorker<N>`.
    pub(crate) worker_id: ComponentId,
}

impl TaskInfo {
    /// Return node and tree ids, from which this task was spawned.
    pub fn source(&self) -> TreeNodeId { self.source }
}

/// Component-wrapper of a singular unit of work for a [`BehaviourNode`].
#[derive(Component, Debug, Reflect)]
#[require(TaskStatus, SleepingTask)]
pub struct TaskWorker<N: BehaviourNode> {
    /// Task that will be used in behaviour systems.
    pub task: N::Task,
}

impl<N: BehaviourNode> TaskWorker<N> {
    /// Create a new task worker.
    pub fn new(task: N::Task) -> Self { Self { task } }
}

impl<N: BehaviourNode> std::ops::Deref for TaskWorker<N> {
    type Target = N::Task;

    fn deref(&self) -> &Self::Target { &self.task }
}

impl<N: BehaviourNode> std::ops::DerefMut for TaskWorker<N> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.task }
}

/// Relationship from a task to the actor (the entity that has
/// [`Behaviour`](crate::core::Behaviour)) it's running for.
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = TaskPool)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct TaskOf(pub(crate) Entity);

impl TaskOf {
    /// Get the actor entity this task is running for.
    pub fn actor(&self) -> Entity { self.0 }
}

/// All of the [`TaskWorker`] tasks currently running for an actor.
#[derive(Component, Reflect, Default, Debug, Clone)]
#[relationship_target(relationship = TaskOf, linked_spawn)]
#[reflect(Component, FromWorld, Default)]
pub struct TaskPool(Vec<Entity>);

/// Relationship to the source task from which this task was queued.
///
/// Most of the time it would be a direct parent of this task's node, but it is
/// not guaranteed (in case of cross-tree tasks, for example).
#[derive(Component, Reflect, Debug, Clone, PartialEq, Eq)]
#[relationship(relationship_target = TaskChildren)]
#[reflect(Component, PartialEq, Debug, Clone)]
pub struct TaskChildOf(pub(crate) Entity);

impl TaskChildOf {
    /// Get the source task entity this task was spawned from.
    pub fn parent(&self) -> Entity { self.0 }
}

/// Tasks that this [`TaskWorker`] spawned.
#[derive(Component, Reflect, Default, Debug, Clone)]
#[relationship_target(relationship = TaskChildOf)]
#[reflect(Component, FromWorld, Default)]
pub struct TaskChildren(Vec<Entity>);

/// Wakefulness of a task: a sleeping task skips update systems until the next
/// behaviour run. This might happen in case of multiple cycles per one run.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SleepingTask {
    /// Task takes part in update systems of the current cycle.
    #[default]
    Awake,
    /// Task skips update systems until the next behaviour run.
    Sleep,
}

impl SleepingTask {
    /// Checks if the task is currently asleep.
    pub fn is_sleeping(&self) -> bool { matches!(self, SleepingTask::Sleep) }
}

/// Special marker to disable any logic of a task.
///
/// A disabling component like bevy's
/// [`Disabled`](bevy::ecs::entity_disabling::Disabled): tasks with it are
/// skipped by every query that doesn't mention it.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct DisabledTask;

/// Entity event for a finished [`TaskWorker`], targeting the task itself.
///
/// Runs right before the finished task is despawned, so observers can still
/// query it for cleanup. Observers can be gated by the node type exactly like
/// lifecycle events are gated by a component: `On<TaskFinished,
/// TaskWorker<WalkNode>>`.
#[derive(EntityEvent, Debug, Clone)]
#[entity_event(trigger = EntityComponentsTrigger<'a>)]
pub struct TaskFinished {
    /// Finished task entity.
    #[event_target]
    pub task: Entity,
    /// Node this task was spawned from.
    pub source: TreeNodeId,
    /// Status with wich task was finished.
    pub status: TaskStatus,
}

/// Entity event for a finished [`TaskWorker`], targeting its source task.
///
/// Observers can be gated by the source node type exactly like lifecycle events
/// are gated by a component: `On<ChildTaskFinished, TaskWorker<WhileNode>>`
/// only runs when the source task belongs to a `WhileNode`.
#[derive(EntityEvent, Debug, Clone)]
#[entity_event(trigger = EntityComponentsTrigger<'a>)]
pub struct ChildTaskFinished {
    /// Parent task that spawned the finished task.
    #[event_target]
    pub task: Entity,
    /// Finished task entity.
    pub child_task: Entity,
    /// Node the finished task was spawned from.
    pub child_source: TreeNodeId,
    /// Status with wich task was finished.
    pub child_status: TaskStatus,
}

/// Entity event for a new [`TaskWorker`], spawned from its source task.
///
/// Observers can be gated by the source node type exactly like lifecycle events
/// are gated by a component: `On<ChildTaskSpawned, TaskWorker<WhileNode>>`
/// only runs when the source task belongs to a `WhileNode`.
#[derive(EntityEvent, Debug, Clone)]
#[entity_event(trigger = EntityComponentsTrigger<'a>)]
pub struct ChildTaskSpawned {
    /// Parent task that spawned the task.
    #[event_target]
    pub parent: Entity,
    /// Spawned task entity.
    pub entity: Entity,
    /// Node the spawned task was spawned from.
    pub source: TreeNodeId,
}

/// System for putting still-running [`TaskWorker`] tasks to sleep until the
/// next real frame. Teardown of finished tasks is handled inline by
/// `on_task_status_inserted_hook` instead of polled here.
fn task_workers_system<N: BehaviourNode>(
    mut q_task: Query<(&TaskStatus, &mut SleepingTask), With<TaskWorker<N>>>,
) {
    for (status, mut sleeping) in &mut q_task {
        if !status.is_finished() {
            sleeping.set_if_neq(SleepingTask::Sleep);
        }
    }
}

fn on_task_status_inserted_hook(
    event: On<Insert, TaskStatus>,
    mut cmd: Commands,
    q_task: Query<(&TaskStatus, &TaskInfo, Option<&TaskChildOf>, Option<&TaskChildren>)>,
    q_child_task: Query<(Entity, &TaskStatus)>,
) -> Result<()> {
    let Ok((status, info, child_of, children)) = q_task.get(event.entity) else {
        return Ok(());
    };
    if !status.is_finished() {
        return Ok(());
    }

    let task = event.entity;
    let info = *info;
    let status = *status;

    cmd.entity(task).try_insert(SleepingTask::Awake);
    cmd.queue(move |world: &mut World| {
        world.trigger_with(
            TaskFinished { task, source: info.source, status },
            EntityComponentsTrigger {
                components: &[info.worker_id],
                old_archetype: None,
                new_archetype: None,
            },
        );
    });

    for (child_task, child_status) in q_child_task.iter_many(children.iter().flat_map(|c| c.iter()))
    {
        if !child_status.is_finished() {
            cmd.entity(child_task).insert(TaskStatus::Success);
        }
    }

    if let Some(parent) = child_of.map(|c| c.parent()) {
        cmd.queue(move |world: &mut World| {
            let Some(parent_worker) = world.get::<TaskInfo>(parent).map(|info| info.worker_id)
            else {
                return;
            };
            if !world.get::<TaskStatus>(parent).is_some_and(|s| !s.is_finished()) {
                return;
            }
            let was_sleeping =
                world.get::<SleepingTask>(parent).is_some_and(SleepingTask::is_sleeping);
            if was_sleeping && let Some(mut sleeping) = world.get_mut::<SleepingTask>(parent) {
                *sleeping = SleepingTask::Awake;
            }
            world.trigger_with(
                ChildTaskFinished {
                    task: parent,
                    child_task: task,
                    child_source: info.source,
                    child_status: status,
                },
                EntityComponentsTrigger {
                    components: &[parent_worker],
                    old_archetype: None,
                    new_archetype: None,
                },
            );
            if was_sleeping && let Some(mut sleeping) = world.get_mut::<SleepingTask>(parent) {
                *sleeping = SleepingTask::Sleep;
            }
        });
    }

    cmd.entity(task).try_despawn();
    Ok(())
}

fn new_run_system(mut q_sleeping: Query<(&TaskStatus, &mut SleepingTask)>) {
    // Finished tasks stay asleep so they go straight to teardown without one more
    // update.
    for (status, mut sleeping) in &mut q_sleeping {
        if !status.is_finished() {
            sleeping.set_if_neq(SleepingTask::Awake);
        }
    }
}
