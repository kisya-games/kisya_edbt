//! [`LoopForNode`] and its structures.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, DecoratorNodeInfo, NodeRef},
    query::TaskMut,
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
};

/// Plugin for [`LoopForNode`] and [`LoopForTask`].
pub struct LoopForNodePlugin;

impl Plugin for LoopForNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<LoopForNode>()
            .with_system(loop_for_node_system)
            .with_child_finish_observer(on_loop_for_child_task_finished_hook)
            .register();
    }
}

/// Loop-like node that runs its child a fixed number of times.
///
/// *LoopForNode* reruns its child until it has succeeded
/// [`iterations`][LoopForNode::iterations] times, then returns
/// [`TaskStatus::Success`]. A failing child stops the loop early and
/// *LoopForNode* returns [`TaskStatus::Failure`].
///
/// **Note**: *LoopForNode* reruns its child once per update so it won't stuck
/// in a deadlock if a child node returns immidiatly.
#[derive(Debug, Reflect, Clone, Copy)]
pub struct LoopForNode {
    /// Number of successful child runs before this node finishes.
    pub iterations: u32,
}

impl Default for LoopForNode {
    fn default() -> Self { Self { iterations: 1 } }
}

impl BehaviourNode for LoopForNode {
    type Info<'a> = DecoratorNodeInfo<'a>;
    type Task = LoopForTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

impl LoopForNode {
    /// Create a new node that runs its child `iterations` times.
    pub fn times(iterations: u32) -> Self { Self { iterations } }
}

/// Task for [`LoopForNode`].
#[derive(Reflect, Default)]
pub struct LoopForTask {
    /// Is decorated task is currently running.
    pub is_running: bool,
    /// Number of times the child has finished successfully so far.
    pub completed: u32,
}

fn loop_for_node_system(
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<LoopForNode>, NodeRef<LoopForNode>)>,
) {
    for (mut task, node) in &mut q_task {
        if task.is_running {
            continue;
        }

        if task.completed >= node.iterations {
            cmd.entity(task.entity()).insert(TaskStatus::Success);
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

fn on_loop_for_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<LoopForNode>>,
    mut cmd: Commands,
    mut q_task: Query<TaskMut<LoopForNode>>,
) {
    let Ok(mut task) = q_task.get_mut(event.task) else {
        return;
    };

    if event.child_status == TaskStatus::Failure {
        cmd.entity(event.task).insert(TaskStatus::Failure);
        return;
    }

    task.completed += 1;
    task.is_running = false;
}
