//! [`WhileNode`] and its structures.

use std::borrow::Cow;

use bevy::prelude::*;
use thiserror::Error;

use crate::{
    core::{
        node::{BehaviourNode, NodeRef, NodeValidationError, TreeNodeId},
        query::TaskMut,
        registrar::BehaviourNodeRegistrarAppExt,
        spawn::SpawnTaskExt,
        task::{ChildTaskFinished, TaskStatus, TaskWorker},
    },
    prelude::{BehaviourNodeInfo, BehaviourTree},
};

/// Plugin for [`WhileNode`] and [`WhileTask`].
pub struct WhileNodePlugin;

impl Plugin for WhileNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<WhileNode>()
            .with_system(while_node_system)
            .with_child_finish_observer(on_while_child_task_finished_hook)
            .register();
    }
}

/// Node that will continiously run an action system only if a condition system
/// continiously return a certain status.
///
/// *WhileNode* works as a `while` loop: it first checks the first child
/// (condition), if it return the same status as [`WhileNode::status`], then it
/// will run the second child (action). Every frame it will run the condition to
/// see if anything changed, and also rerun the action if it's event.
///
/// If condition at some point return different status that is required, then
/// *WhileNode* returns [`TaskStatus::Success`].
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct WhileNode {
    /// Status that this node will use to determine if it should continue or
    /// halt.
    pub status: TaskStatus,
}

impl BehaviourNode for WhileNode {
    type Info<'a> = WhileNodeInfo<'a>;
    type Task = WhileTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

impl WhileNode {
    /// Create a node that will run while [`TaskStatus::Success`].
    pub fn while_success() -> Self { Self { status: TaskStatus::Success } }

    /// Create a node that will run while [`TaskStatus::Failure`].
    pub fn while_failure() -> Self { Self { status: TaskStatus::Failure } }
}

/// Errors for [`WhileTask`].
#[derive(Error, Debug)]
enum WhileTaskError {
    #[error("WhileNode doesn't have a condition child ({0:?})")]
    NoConditionChild(Entity),
    #[error("WhileNode doesn't have a action child ({0:?})")]
    NoActionChild(Entity),
    #[error("Unexpected WhileNode state({0:?})")]
    UnexpectedState(Entity),
}

#[derive(Debug, Reflect, Default)]
enum WhileTaskState {
    #[default]
    Initial,
    ConditionRun {
        condition_node_id: TreeNodeId,
    },
    ActionRun {
        action_node_id: TreeNodeId,
    },
    ConditionAndActionRun {
        condition_node_id: TreeNodeId,
        action_node_id: TreeNodeId,
    },
    Done,
}

/// Task for [`WhileNode`].
#[derive(Debug, Reflect, Default)]
pub struct WhileTask {
    state: WhileTaskState,
}

fn while_node_system(
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<WhileNode>, NodeRef<WhileNode>)>,
) {
    for (mut task, node) in &mut q_task {
        if node.info().is_empty() {
            cmd.entity(task.entity()).insert(TaskStatus::Success);
            continue;
        }

        let result = || -> Result<(), WhileTaskError> {
            match task.state {
                WhileTaskState::Initial => {
                    let condition_node_id = node
                        .info()
                        .get_child(0)
                        .ok_or_else(|| WhileTaskError::NoConditionChild(task.actor()))?;

                    cmd.entity(task.entity()).spawn_task(condition_node_id);
                    task.state = WhileTaskState::ConditionRun { condition_node_id };
                },
                WhileTaskState::ActionRun { action_node_id } => {
                    let condition_node_id = node
                        .info()
                        .get_child(0)
                        .ok_or_else(|| WhileTaskError::NoConditionChild(task.actor()))?;

                    cmd.entity(task.entity()).spawn_task(condition_node_id);
                    task.state =
                        WhileTaskState::ConditionAndActionRun { condition_node_id, action_node_id };
                },
                _ => {},
            }
            Ok(())
        }();

        if let Err(err) = result {
            cmd.entity(task.entity()).insert(TaskStatus::Failure);
            error!("Error while trying to update a WhileTask: {err}");
            continue;
        }
    }
}

fn on_while_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<WhileNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<WhileNode>, NodeRef<WhileNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.task) else {
        return;
    };

    let result = || -> Result<(), WhileTaskError> {
        match task.state {
            WhileTaskState::ConditionRun { condition_node_id }
                if condition_node_id == event.child_source =>
            {
                if event.child_status == node.status {
                    let action_node_id = node
                        .info()
                        .get_child(1)
                        .ok_or_else(|| WhileTaskError::NoActionChild(task.actor()))?;

                    cmd.entity(event.task).spawn_task(action_node_id);
                    task.state = WhileTaskState::ActionRun { action_node_id };
                } else {
                    task.state = WhileTaskState::Done;
                    cmd.entity(event.task).insert(TaskStatus::Success);
                }
            },
            WhileTaskState::ActionRun { action_node_id }
                if action_node_id == event.child_source =>
            {
                task.state = WhileTaskState::Initial;
            },
            WhileTaskState::ConditionAndActionRun { condition_node_id, action_node_id }
                if condition_node_id == event.child_source =>
            {
                if event.child_status == node.status {
                    task.state = WhileTaskState::ActionRun { action_node_id };
                } else {
                    task.state = WhileTaskState::Done;
                    cmd.entity(event.task).insert(TaskStatus::Success);
                }
            },
            WhileTaskState::ConditionAndActionRun { action_node_id, condition_node_id }
                if action_node_id == event.child_source =>
            {
                task.state = WhileTaskState::ConditionRun { condition_node_id };
            },
            WhileTaskState::Done => {},
            _ => Err(WhileTaskError::UnexpectedState(task.actor()))?,
        }
        Ok(())
    }();

    if let Err(err) = result {
        cmd.entity(event.task).insert(TaskStatus::Failure);
        error!("Error while finishing a WhileTask child: {err}");
    }
}

/// Info for [`WhileNode`].
#[derive(Clone, Copy)]
pub struct WhileNodeInfo<'a> {
    id: TreeNodeId,
    tree: &'a BehaviourTree,
}

impl<'a> BehaviourNodeInfo<'a> for WhileNodeInfo<'a> {
    fn from_id_and_tree(id: TreeNodeId, tree: &'a BehaviourTree) -> Self { Self { id, tree } }

    fn validate(&self) -> Result<(), NodeValidationError> {
        if self.len() != 2 {
            return Err(NodeValidationError::InvalidChildCount(self.len(), 2));
        }

        Ok(())
    }

    fn available_slots(&self) -> usize { 2 }

    fn slot_name(&self, index: usize) -> Option<Cow<'static, str>> {
        match index {
            0 => Some(Cow::Borrowed("Loop Header")),
            1 => Some(Cow::Borrowed("Loop Body")),
            _ => None,
        }
    }
}

impl WhileNodeInfo<'_> {
    /// Return size of available children nodes.
    pub fn len(&self) -> usize { self.tree.get_children_len(self.id.node) }

    /// Check if there are no children.
    pub fn is_empty(&self) -> bool { self.len() == 0 }

    /// Try to get a child node at `index`.
    pub fn get_child(&self, index: usize) -> Option<TreeNodeId> {
        self.tree
            .get_child_id(self.id.node, index)
            .map(|node| TreeNodeId { node, tree: self.id.tree })
    }
}
