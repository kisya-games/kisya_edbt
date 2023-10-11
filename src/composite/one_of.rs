//! [`OneOfNode`] and its structures.

use bevy::prelude::*;
use rand::{distributions::WeightedIndex, prelude::*, seq::IteratorRandom};

use crate::core::{
    node::{BehaviourNode, CompositeNodeInfo, NodeRef},
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
};

/// Plugin for [`OneOfNode`].
pub struct OneOfNodePlugin;

impl Plugin for OneOfNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<OneOfNode>()
            .with_setup_observer(on_one_of_setup_hook)
            .with_child_finish_observer(on_one_of_child_task_finished_hook)
            .register();
    }
}

/// Policy to run [`OneOfNode`].
#[derive(Debug, Default, Reflect, Clone)]
pub enum OneOfPolicy {
    /// Select a child randomly.
    #[default]
    Random,
    /// Select a child randomly with a weighted distribution.
    WeightedRandom(Vec<f32>),
}

/// Node that will randomly choose and run only one of its children.
///
/// **Note**: `OneOfNode` simply uses [rand](https://github.com/rust-random/rand) to choose
/// one of its children to run.
#[derive(Debug, Default, Reflect, Clone)]
pub struct OneOfNode {
    /// Policy used to determine what child to run.
    pub policy: OneOfPolicy,
}

impl BehaviourNode for OneOfNode {
    type Info<'a> = CompositeNodeInfo<'a>;
    type Task = ();

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

impl OneOfNode {
    /// Create a node with a [`OneOfPolicy::WeightedRandom`] policy.
    pub fn weighted(weights: Vec<f32>) -> Self {
        Self { policy: OneOfPolicy::WeightedRandom(weights) }
    }

    /// Create a node with a [`OneOfPolicy::Random`] policy.
    pub fn random() -> Self { Self { policy: OneOfPolicy::Random } }
}

fn on_one_of_setup_hook(
    event: On<Add, TaskWorker<OneOfNode>>,
    mut cmd: Commands,
    q_task: Query<NodeRef<OneOfNode>>,
) {
    let Ok(node) = q_task.get(event.entity) else {
        return;
    };

    let node_id = match node.policy {
        OneOfPolicy::Random => {
            let mut rng = rand::thread_rng();
            node.info().iter().choose(&mut rng)
        },
        OneOfPolicy::WeightedRandom(ref weights) => {
            let mut rng = rand::thread_rng();
            WeightedIndex::new(weights)
                .ok()
                .and_then(|dist| node.info().iter().nth(dist.sample(&mut rng)))
        },
    };

    if let Some(node_id) = node_id {
        cmd.entity(event.entity).spawn_task(node_id);
    } else {
        cmd.entity(event.entity).insert(TaskStatus::Success);
    }
}

fn on_one_of_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<OneOfNode>>,
    mut cmd: Commands,
) {
    cmd.entity(event.task).insert(event.child_status);
}
