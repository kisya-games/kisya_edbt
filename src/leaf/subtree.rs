//! [`SubtreeNode`] and related structs.

use bevy::{asset::AssetPath, prelude::*};

use crate::{
    core::{
        node::{BehaviourNode, LeafNodeInfo, NodeRef, TreeNodeId},
        registrar::BehaviourNodeRegistrarAppExt,
        task::{ChildTaskFinished, TaskStatus, TaskWorker},
        tree::BehaviourTree,
    },
    prelude::SpawnTaskExt,
};

/// Plugin for [`SubtreeNode`].
pub struct SubtreeNodePlugin;

impl Plugin for SubtreeNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<SubtreeNode>()
            .with_setup_observer(on_subtree_setup_hook)
            .with_child_finish_observer(on_subtree_child_task_finished_hook)
            .register();
    }
}

/// Source from which subtree can be loaded.
// TODO: use bevy's HandleTemplate / HandleReference ?
#[derive(Debug, Reflect, Clone)]
pub enum SubtreeSource {
    /// Use asset handle as a source.
    Handle(Handle<BehaviourTree>),
    /// Use asset path as a source.
    Path(AssetPath<'static>),
}

impl Default for SubtreeSource {
    fn default() -> Self { Self::Handle(Handle::default()) }
}

/// Node that will simply run a subtree root node as a child
/// of this node. It will be [`TaskStatus::Running`] until the child
/// node is done; and will return what child returns.
#[derive(Debug, Default, Reflect, Clone)]
pub struct SubtreeNode {
    /// Source of a subtree to run.
    pub source: SubtreeSource,
}

impl SubtreeNode {
    /// Create a new [`SubtreeNode`] from a tree handle.
    pub fn from_handle(handle: Handle<BehaviourTree>) -> Self {
        Self { source: SubtreeSource::Handle(handle) }
    }

    /// Create a new [`SubtreeNode`] from an asset path.
    pub fn from_path(path: impl Into<AssetPath<'static>>) -> Self {
        Self { source: SubtreeSource::Path(path.into()) }
    }
}

impl BehaviourNode for SubtreeNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ();

    fn build_task(&self) -> Self::Task { () }
}

fn on_subtree_setup_hook(
    event: On<Add, TaskWorker<SubtreeNode>>,
    mut cmd: Commands,
    trees: Res<Assets<BehaviourTree>>,
    asset_server: Res<AssetServer>,
    q_task: Query<NodeRef<SubtreeNode>>,
) {
    let Ok(node) = q_task.get(event.entity) else {
        return;
    };

    let handle = match &node.source {
        SubtreeSource::Handle(handle) => Some(handle.clone()),
        SubtreeSource::Path(path) => asset_server.get_handle(path),
    };

    match handle.and_then(|handle| trees.get(&handle).map(|tree| (handle.id(), tree))) {
        Some((tree_id, tree)) => {
            let tree_node_id = TreeNodeId { node: tree.get_root_id(), tree: tree_id };
            cmd.entity(event.entity).spawn_task(tree_node_id);
        },
        None => {
            error!("Unknown SubtreeNode source: {:?}", node.source);
            cmd.entity(event.entity).insert(TaskStatus::Failure);
        },
    }
}

fn on_subtree_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<SubtreeNode>>,
    mut cmd: Commands,
) {
    cmd.entity(event.task).insert(event.child_status);
}
