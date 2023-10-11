//! Tools to spawn child tasks in a running [`BehaviourTree`].

use bevy::{
    ecs::{
        event::EntityComponentsTrigger,
        system::{EntityCommand, EntityCommands},
    },
    prelude::*,
    reflect::FromType,
};
use thiserror::Error;

use crate::core::{
    DisabledBehaviour,
    node::{BehaviourNode, TreeNodeId},
    runner::BehaviourRunnerCycles,
    task::{ChildTaskSpawned, SleepingTask, TaskChildOf, TaskInfo, TaskOf, TaskStatus, TaskWorker},
    tree::BehaviourTree,
};

/// Node payload used in [`SpawnTask`] and [`SpawnTaskInput`].
pub type SpawnTaskPayload = smallvec::SmallVec<[(Entity, TreeNodeId); 2]>;

/// Errors for [`SpawnTask`].
#[derive(Error, Debug, Clone)]
pub enum TaskSpawnError {
    /// [`BehaviourTree`] is not present in bevy's asset system.
    #[error("Behaviour tree is not valid asset ({0})")]
    InvalidTree(AssetId<BehaviourTree>),
    /// Parent task isn't in a right condition.
    #[error("Parent task {0} is invalid")]
    InvalidParentTask(Entity),
    /// Dynamic node type mismatched. In a healthy system, this should never
    /// happen.
    #[error("Node type mismatch (critical error)")]
    NodeTypeMismatch,
    /// Task isn't related to any actor.
    #[error("Task {0} isn't related to any actor")]
    NotAnActorTask(Entity),
    /// Entity wasn't found in [`World`].
    #[error("Entity {0} not found")]
    EntityNotFound(Entity),
    /// Status is neither [`TaskStatus::Success`] nor [`TaskStatus::Failure`].
    #[error("Status used for finishing tasks is {0}. Must be either Success or Failure")]
    NotFinishStatus(TaskStatus),
}

/// Trait to group different inputs available for [`SpawnTaskExt::spawn_task`].
pub trait SpawnTaskInput {
    /// Mapping of input to a collection of [`Entity`].
    type Out;

    /// Zip [`Self::Out`] and self to payload.
    fn into_payload(self, pre_spawn: &Self::Out) -> SpawnTaskPayload;
    /// Pre-spawn [`Self::Out`] collection.
    fn pre_spawn(&self, cmd: &mut Commands) -> Self::Out;
}

impl SpawnTaskInput for TreeNodeId {
    type Out = Entity;

    fn into_payload(self, pre_spawn: &Self::Out) -> SpawnTaskPayload {
        smallvec::smallvec![(*pre_spawn, self)]
    }

    fn pre_spawn(&self, cmd: &mut Commands) -> Self::Out { cmd.spawn_empty().id() }
}

impl<const N: usize> SpawnTaskInput for [TreeNodeId; N] {
    type Out = [Entity; N];

    fn into_payload(self, pre_spawn: &Self::Out) -> SpawnTaskPayload {
        smallvec::SmallVec::from_const(std::array::from_fn(|i| (pre_spawn[i], self[i])))
    }

    fn pre_spawn(&self, cmd: &mut Commands) -> Self::Out {
        std::array::from_fn(|_i| cmd.spawn_empty().id())
    }
}

impl<'a> SpawnTaskInput for &'a [TreeNodeId] {
    type Out = Vec<Entity>;

    fn into_payload(self, pre_spawn: &Self::Out) -> SpawnTaskPayload {
        smallvec::SmallVec::from_iter(pre_spawn.iter().copied().zip(self.into_iter().copied()))
    }

    fn pre_spawn(&self, cmd: &mut Commands) -> Self::Out {
        std::iter::repeat_with(|| cmd.spawn_empty().id()).take(self.len()).collect()
    }
}

/// Extension trait for [`Commands`] to use [`SpawnTask`] from parent tasks
/// easily.
///
/// *NOTE: `spawn_tasks`/`spawn_task` should be called on task entities, not
/// actor entities.*
pub trait SpawnTaskExt {
    /// Spawn a new task or multiple tasks with `input`:
    /// - Single [`TreeNodeId`] spawn one task and return [`Entity`].
    /// - `[TreeNodeId; N]` spawn batch of tasks and return `[Entity; N]`.
    /// - `&[TreeNodeId]` spawn batch of tasks and return `Vec<Entity>`.
    fn spawn_task<I>(&mut self, input: I) -> I::Out
    where
        I: SpawnTaskInput;
}

impl SpawnTaskExt for EntityCommands<'_> {
    fn spawn_task<I>(&mut self, input: I) -> I::Out
    where
        I: SpawnTaskInput,
    {
        let parent = self.id();
        let mut cmd = self.commands();
        let pre_spawned = input.pre_spawn(&mut cmd);
        let payload = input.into_payload(&pre_spawned);
        cmd.queue(move |world: &mut World| -> Result<()> {
            let actor = world
                .get::<TaskOf>(parent)
                .map(TaskOf::actor)
                .ok_or_else(|| TaskSpawnError::NotAnActorTask(parent))?;
            if !world.entities().contains_spawned(actor) {
                return Ok(());
            }
            let spawn = SpawnTask { parent: Some(parent), payload };
            spawn.apply(world.entity_mut(actor))
        });
        pre_spawned
    }
}

/// Command to spawn task children for the actor.
#[derive(Debug, Clone)]
pub struct SpawnTask {
    /// Array of pre-generated `Entity` and `TreeNodeId` to create tasks from.
    pub payload: SpawnTaskPayload,
    /// Task [`Entity`] that queued new tasks. May be `None` in case of root
    /// nodes.
    pub parent: Option<Entity>,
}

impl EntityCommand for SpawnTask {
    type Out = Result<()>;

    fn apply(self, entity: EntityWorldMut) -> Result<()> {
        let actor = entity.id();
        let world = entity.into_world_mut();
        let registry = world.resource::<AppTypeRegistry>().clone();
        let registry = registry.0.read();

        for (task_entity, tree_node_id) in self.payload {
            // A source can finish and despawn partway through spawning its batch of
            // children; don't spawn a child against a dead source.
            if self.parent.is_some_and(|parent| world.get_entity(parent).is_err()) {
                break;
            }

            if let Some(methods) =
                registry.get_type_data::<DynamicNodeSpawner>(tree_node_id.node.type_id)
            {
                (methods.spawn)(world, actor, task_entity, self.parent, tree_node_id)?;
            }
        }

        if world.get::<DisabledBehaviour>(actor).is_none() {
            // TODO: this is probably should work on observers in runner.rs ?
            world.resource_mut::<BehaviourRunnerCycles>().rerun = true;
        }

        Ok(())
    }
}

/// Reflection data for a typed node to spawn itself onto an entity.
#[derive(Clone)]
pub(crate) struct DynamicNodeSpawner {
    spawn: fn(&mut World, Entity, Entity, Option<Entity>, TreeNodeId) -> Result<()>,
}

impl<N: BehaviourNode> FromType<N> for DynamicNodeSpawner {
    fn from_type() -> Self { Self { spawn: spawn_task::<N> } }
}

fn spawn_task<N: BehaviourNode>(
    world: &mut World,
    actor: Entity,
    task_entity: Entity,
    parent: Option<Entity>,
    tree_node_id: TreeNodeId,
) -> Result<()> {
    let task = {
        let trees = world.resource::<Assets<BehaviourTree>>();

        let tree =
            trees.get(tree_node_id.tree).ok_or(TaskSpawnError::InvalidTree(tree_node_id.tree))?;

        let node = tree
            .get_node(tree_node_id.node)
            .and_then(|dynamic_node| dynamic_node.downcast::<N>())
            .ok_or(TaskSpawnError::NodeTypeMismatch)?;

        node.build_task()
    };
    let worker_id = world.register_component::<TaskWorker<N>>();

    // `TaskOf`/`TaskChildOf` go in first to trigger their hooks.
    let mut task_mut = world.get_entity_mut(task_entity)?;
    task_mut.insert(TaskOf(actor));
    if let Some(source) = parent {
        task_mut.insert(TaskChildOf(source));
    }
    task_mut.insert((
        Name::new(N::short_type_path()),
        TaskInfo { source: tree_node_id, worker_id },
        TaskWorker::<N>::new(task),
    ));

    if let Some(parent) = parent
        && let Ok(mut parent_mut) = world.get_entity_mut(parent)
    {
        let parent_worker_id = parent_mut
            .get::<TaskInfo>()
            .map(|info| info.worker_id)
            .ok_or_else(|| TaskSpawnError::InvalidParentTask(parent))?;

        // Remove sleeping so observers can use `TaskRef`/`TaskMut`
        let was_sleeping = parent_mut.contains::<SleepingTask>();
        if was_sleeping {
            parent_mut.remove::<SleepingTask>();
        }
        parent_mut.world_scope(|world| {
            world.trigger_with(
                ChildTaskSpawned { parent, entity: task_entity, source: tree_node_id },
                EntityComponentsTrigger {
                    components: &[parent_worker_id],
                    old_archetype: None,
                    new_archetype: None,
                },
            );
        });
        if was_sleeping && parent_mut.is_spawned() {
            parent_mut.insert(SleepingTask);
        }
    }

    Ok(())
}
