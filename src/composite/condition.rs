//! [`ConditionNode`] and its structures.

use std::borrow::Cow;

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, BehaviourNodeInfo, NodeRef, NodeValidationError, TreeNodeId},
    query::TaskMut,
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
    tree::BehaviourTree,
};

/// Plugin for [`ConditionNode`] and [`ConditionTask`].
pub struct ConditionNodePlugin;

impl Plugin for ConditionNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<ConditionNode>()
            .with_setup_observer(on_condition_setup_hook)
            .with_child_finish_observer(on_condition_child_task_finished_hook)
            .register();
    }
}

/// Node with a conditional rule to run one of two branches.
///
/// *ConditionNode* works as `if` construction with 2 or 3 children:
/// - The first child is a condition, depending on result of which it will
///   decide what to do next.
/// - If condition child returned [`TaskStatus::Success`], then run the second
///   child (true branch) and return its status.
/// - If condition child returned [`TaskStatus::Failure`], then run the third
///   child (optional else branch) and return its status, or return
///   [`TaskStatus::Failure`] if there is none.
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct ConditionNode;

impl BehaviourNode for ConditionNode {
    type Info<'a> = ConditionNodeInfo<'a>;
    type Task = ConditionTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

/// Info for [`ConditionNode`].
#[derive(Clone, Copy)]
pub struct ConditionNodeInfo<'a> {
    id: TreeNodeId,
    tree: &'a BehaviourTree,
}

impl<'a> BehaviourNodeInfo<'a> for ConditionNodeInfo<'a> {
    fn from_id_and_tree(id: TreeNodeId, tree: &'a BehaviourTree) -> Self { Self { id, tree } }

    fn validate(&self) -> Result<(), NodeValidationError> {
        match self.len() {
            2 | 3 => Ok(()),
            len if len > 3 => Err(NodeValidationError::TooMuchChildren(len, 3)),
            len => Err(NodeValidationError::TooFewChildren(len, 2)),
        }
    }

    fn available_slots(&self) -> usize { 3 }

    fn slot_name(&self, index: usize) -> Option<Cow<'static, str>> {
        match index {
            0 => Some(Cow::Borrowed("If")),
            1 => Some(Cow::Borrowed("Then Branch")),
            2 => Some(Cow::Borrowed("Else Branch")),
            _ => None,
        }
    }
}

impl ConditionNodeInfo<'_> {
    /// Return size of available children nodes.
    pub fn len(&self) -> usize { self.tree.get_children_len(self.id.node) }

    /// Try to get a child node at `index`.
    pub fn get_child(&self, index: usize) -> Option<TreeNodeId> {
        self.tree
            .get_child_id(self.id.node, index)
            .map(|node| TreeNodeId { node, tree: self.id.tree })
    }
}

/// Task for [`ConditionNode`].
#[derive(Default, Reflect)]
pub struct ConditionTask {
    was_condition_run: bool,
}

fn on_condition_setup_hook(
    event: On<Add, TaskWorker<ConditionNode>>,
    mut cmd: Commands,
    q_task: Query<NodeRef<ConditionNode>>,
) {
    let Ok(node) = q_task.get(event.entity) else {
        return;
    };

    match node.info().get_child(0) {
        Some(node_id) => {
            cmd.entity(event.entity).spawn_task(node_id);
        },
        None => {
            cmd.entity(event.entity).insert(TaskStatus::Success);
        },
    };
}

fn on_condition_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<ConditionNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<ConditionNode>, NodeRef<ConditionNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.task) else {
        return;
    };

    match (event.child_status, task.was_condition_run) {
        (condition_status @ (TaskStatus::Success | TaskStatus::Failure), false) => {
            let index = if condition_status == TaskStatus::Success { 1 } else { 2 };

            if let Some(node_id) = node.info().get_child(index) {
                cmd.entity(event.task).spawn_task(node_id);
            } else {
                cmd.entity(event.task).insert(condition_status);
            }
            task.was_condition_run = true;
        },
        (child_status @ (TaskStatus::Success | TaskStatus::Failure), true) => {
            cmd.entity(event.task).insert(child_status);
        },
        _ => {
            error!("Unexpected state of ConditionNode");
        },
    };
}
