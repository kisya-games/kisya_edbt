//! [`NotNode`] and its structures.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, DecoratorNodeInfo, NodeRef},
    query::TaskRef,
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, TaskStatus, TaskWorker},
};

/// Plugin for [`NotNode`] and `NotTask`.
pub struct NotNodePlugin;

impl Plugin for NotNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<NotNode>()
            .with_setup_observer(on_not_setup_hook)
            .with_child_finish_observer(on_not_child_task_finished_hook)
            .register();
    }
}

/// Simple node that will run its child and return negated status.
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct NotNode;

impl BehaviourNode for NotNode {
    type Info<'a> = DecoratorNodeInfo<'a>;
    type Task = ();

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

fn on_not_setup_hook(
    event: On<Add, TaskWorker<NotNode>>,
    mut cmd: Commands,
    q_task: Query<(TaskRef<NotNode>, NodeRef<NotNode>)>,
) {
    let Ok((_, node)) = q_task.get(event.entity) else {
        return;
    };
    if let Some(node_id) = node.info().get_child() {
        cmd.entity(event.entity).spawn_task(node_id);
    } else {
        error!("No child for NotNode");
        cmd.entity(event.entity).insert(TaskStatus::Failure);
    }
}

fn on_not_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<NotNode>>,
    mut cmd: Commands,
) {
    let status = match event.child_status {
        TaskStatus::Running => {
            error!("NotNode child finished with Running");
            TaskStatus::Failure
        },
        TaskStatus::Success => TaskStatus::Failure,
        TaskStatus::Failure => TaskStatus::Success,
    };
    cmd.entity(event.task).insert(status);
}
