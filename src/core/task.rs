//! [`BehaviourNode's`][BehaviourNode] task components and plugins for bevy ecs.

use std::marker::PhantomData;

use bevy::{
    ecs::{component::ComponentId, event::EntityComponentsTrigger, query::Has},
    prelude::*,
    reflect::GetTypeRegistration,
};

use crate::core::{
    node::{BehaviourNode, TreeNodeId},
    runner::{BehaviourNewRunSchedule, BehaviourUpdateCycleSchedule, BehaviourUpdateCycleSystems},
    spawn::DynamicNodeSpawner,
    tree::ReflectDynamicNode,
};

/// Plugin for general task logic.
pub struct TaskPlugin;

impl Plugin for TaskPlugin {
    fn build(&self, app: &mut App) {
        app.world_mut().register_disabling_component::<DisabledTask>();

        app.add_systems(BehaviourNewRunSchedule, new_run_system);
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
            task_workers_system::<N>.in_set(BehaviourUpdateCycleSystems::PostUpdate),
        );
        app.add_observer(on_task_status_inserted_hook::<N>);
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
#[require(TaskStatus)]
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

/// Marker for a task that skips update systems until the next behaviour run.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[component(immutable)]
pub struct SleepingTask;

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
    mut cmd: Commands,
    q_task: Query<(Entity, &TaskStatus, Has<SleepingTask>), With<TaskWorker<N>>>,
) {
    for (entity, status, is_sleeping) in &q_task {
        if !status.is_finished() && !is_sleeping {
            cmd.entity(entity).try_insert(SleepingTask);
        }
    }
}

fn on_task_status_inserted_hook<N: BehaviourNode>(
    event: On<Insert, TaskStatus>,
    mut cmd: Commands,
    q_task: Query<
        (&TaskStatus, &TaskInfo, Option<&TaskChildOf>, Option<&TaskChildren>),
        With<TaskWorker<N>>,
    >,
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

    cmd.entity(task).remove::<SleepingTask>();
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
            let was_sleeping = world.get::<SleepingTask>(parent).is_some();
            if was_sleeping {
                world.entity_mut(parent).remove::<SleepingTask>();
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
            if was_sleeping && world.get_entity(parent).is_ok() {
                world.entity_mut(parent).insert(SleepingTask);
            }
        });
    }

    cmd.entity(task).try_despawn();
    Ok(())
}

fn new_run_system(
    mut commands: Commands,
    q_sleeping: Query<(Entity, &TaskStatus), With<SleepingTask>>,
) {
    // Finished tasks stay asleep so they go straight to teardown without one more
    // update.
    for (entity, status) in &q_sleeping {
        if !status.is_finished() {
            commands.entity(entity).remove::<SleepingTask>();
        }
    }
}
