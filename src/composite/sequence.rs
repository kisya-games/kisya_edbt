//! [`SequenceNode`] and its structures.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, CompositeNodeInfo, NodeRef},
    query::{TaskMut, TaskRef},
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
};

/// Plugin for [`SequenceNode`] and [`SequenceTask`].
pub struct SequenceNodePlugin;

impl Plugin for SequenceNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<SequenceNode>()
            .with_setup_observer(on_sequence_setup_hook)
            .with_child_finish_observer(on_sequence_child_task_finished_hook)
            .register();
    }
}

/// Node that will spawn its children one by one until one of them return
/// [`TaskStatus::Failure`].
///
/// *SequenceNode* works with these rules:
/// - If the children task return [`TaskStatus::Failure`], *SequenceNode* return
///   [`TaskStatus::Failure`] as well.
/// - If there are no nodes anymore to run, *SequenceNode* return
///   [`TaskStatus::Success`].
/// - Otherwise, it will be [`TaskStatus::Running`].
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct SequenceNode;

impl BehaviourNode for SequenceNode {
    type Info<'a> = CompositeNodeInfo<'a>;
    type Task = SequenceTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

/// Task for [`SequenceNode`].
#[derive(Default, Reflect)]
pub struct SequenceTask {
    /// Current index of a running child.
    pub current: usize,
}

fn on_sequence_setup_hook(
    event: On<Add, TaskWorker<SequenceNode>>,
    mut cmd: Commands,
    q_task: Query<(TaskRef<SequenceNode>, NodeRef<SequenceNode>)>,
) {
    let Ok((task, node)) = q_task.get(event.entity) else {
        return;
    };

    match node.info().get_child(task.current) {
        Some(node_id) => {
            cmd.entity(event.entity).spawn_task(node_id);
        },
        None => {
            cmd.entity(event.entity).insert(TaskStatus::Success);
        },
    };
}

fn on_sequence_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<SequenceNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<SequenceNode>, NodeRef<SequenceNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.task) else {
        return;
    };

    match event.child_status {
        TaskStatus::Success => {
            task.current += 1;

            if let Some(node_id) = node.info().get_child(task.current) {
                cmd.entity(event.task).spawn_task(node_id);
            } else {
                cmd.entity(event.task).insert(TaskStatus::Success);
            }
        },
        TaskStatus::Failure => {
            cmd.entity(event.task).insert(TaskStatus::Failure);
        },
        TaskStatus::Running => {
            error!("Unexpected state of SequenceNode");
        },
    };
}
