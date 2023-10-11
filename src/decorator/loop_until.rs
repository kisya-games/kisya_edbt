//! [`LoopUntilNode`] and its structures.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, DecoratorNodeInfo, NodeRef},
    query::TaskMut,
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
};

/// Plugin for [`LoopUntilNode`] and [`LoopUntilTask`].
pub struct LoopUntilNodePlugin;

impl Plugin for LoopUntilNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<LoopUntilNode>()
            .with_system(loop_until_node_system)
            .with_child_finish_observer(on_loop_until_child_task_finished_hook)
            .register();
    }
}

/// Loop-like node that will repeatedly run its child until it return a certain
/// status.
///
/// *LoopUntilNode* works as infinite loop with one rule to break it: child node
/// must return [`LoopUntilNode::status`], then *LoopUntilNode* will return
/// [`TaskStatus::Success`].
///
/// **Note**: *LoopUntilNode* will rerun its child once per update so it won't
/// stuck in a deadlock if a child node returns immidiatly.
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct LoopUntilNode {
    /// Status for child node on which this node will finish.
    pub status: TaskStatus,
}

impl BehaviourNode for LoopUntilNode {
    type Info<'a> = DecoratorNodeInfo<'a>;
    type Task = LoopUntilTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

impl LoopUntilNode {
    /// Create a new node that will run until child returns
    /// [`TaskStatus::Success`].
    pub fn until_success() -> Self { Self { status: TaskStatus::Success } }

    /// Create a new node that will run until child returns
    /// [`TaskStatus::Failure`].
    pub fn until_failure() -> Self { Self { status: TaskStatus::Failure } }
}

/// Task for [`LoopUntilNode`].
#[derive(Reflect, Default)]
pub struct LoopUntilTask {
    /// Is decorated task is currently running.
    pub is_running: bool,
}

fn loop_until_node_system(
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<LoopUntilNode>, NodeRef<LoopUntilNode>)>,
) {
    for (mut task, node) in &mut q_task {
        if task.is_running {
            continue;
        }

        if let Some(node_id) = node.info().get_child() {
            cmd.entity(task.entity()).spawn_task(node_id);
            task.is_running = true;
        } else {
            cmd.entity(task.entity()).insert(TaskStatus::Failure);
        }
    }
}

fn on_loop_until_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<LoopUntilNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<LoopUntilNode>, NodeRef<LoopUntilNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.task) else {
        return;
    };

    if event.child_status == node.status {
        cmd.entity(event.task).insert(TaskStatus::Success);
        return;
    }

    task.is_running = false;
}
