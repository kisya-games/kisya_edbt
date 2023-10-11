//! [`ConstNode`] and related structs.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, LeafNodeInfo, NodeRef},
    registrar::BehaviourNodeRegistrarAppExt,
    task::{TaskStatus, TaskWorker},
};

/// Plugin for [`ConstNode`].
pub struct ConstNodePlugin;

impl Plugin for ConstNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<ConstNode>().with_setup_observer(on_const_setup_hook).register();
    }
}

/// Node that finishes immediately with a fixed status and no side effects.
#[derive(Debug, Reflect, Clone, Copy)]
pub struct ConstNode {
    /// Status this node finishes with.
    pub status: TaskStatus,
}

impl Default for ConstNode {
    fn default() -> Self { Self::success() }
}

impl ConstNode {
    /// Create a node that immediately succeeds.
    pub fn success() -> Self { Self { status: TaskStatus::Success } }

    /// Create a node that immediately fails.
    pub fn failure() -> Self { Self { status: TaskStatus::Failure } }
}

impl BehaviourNode for ConstNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ();

    fn build_task(&self) -> Self::Task {}
}

fn on_const_setup_hook(
    event: On<Add, TaskWorker<ConstNode>>,
    mut cmd: Commands,
    q_task: Query<NodeRef<ConstNode>>,
) {
    let Ok(node) = q_task.get(event.entity) else {
        return;
    };
    cmd.entity(event.entity).insert(node.status);
}
