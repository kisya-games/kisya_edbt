//! [`LoopNode`] and its structures.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, DecoratorNodeInfo, NodeRef},
    query::TaskMut,
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
};

/// Plugin for [`LoopNode`] and [`LoopTask`].
pub struct LoopNodePlugin;

impl Plugin for LoopNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<LoopNode>()
            .with_system(loop_node_system)
            .with_child_finish_observer(on_loop_child_task_finished_hook)
            .register();
    }
}

/// Simple node that will rerun its child every time it finishes.
///
/// One property of *LoopNode* is that it will never finishes. It's usefull to
/// create infinite tasks from nodes that would otherwise return to the parent.
///
/// **Note**: *Loop* will rerun its child once per update so it won't stuck in a
/// deadlock if a child node returns immediately.
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct LoopNode;

impl BehaviourNode for LoopNode {
    type Info<'a> = DecoratorNodeInfo<'a>;
    type Task = LoopTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

/// Task for [`LoopNode`].
#[derive(Default, Reflect)]
pub struct LoopTask {
    /// Is decorated task is currently running.
    pub is_running: bool,
}

fn loop_node_system(mut cmd: Commands, mut q_task: Query<(TaskMut<LoopNode>, NodeRef<LoopNode>)>) {
    for (mut task, node) in &mut q_task {
        if task.is_running {
            continue;
        }

        if let Some(node_id) = node.info().get_child() {
            cmd.entity(task.entity()).spawn_task(node_id);
            task.is_running = true;
        } else {
            error!("No child for LoopNode");
            cmd.entity(task.entity()).insert(TaskStatus::Failure);
        }
    }
}

fn on_loop_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<LoopNode>>,
    mut q_task: Query<TaskMut<LoopNode>>,
) {
    let Ok(mut task) = q_task.get_mut(event.task) else {
        return;
    };

    task.is_running = false;
}
