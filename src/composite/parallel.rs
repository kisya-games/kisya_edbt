//! [`ParallelNode`] and its structures.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, CompositeNodeInfo, NodeRef},
    query::TaskMut,
    registrar::BehaviourNodeRegistrarAppExt,
    spawn::SpawnTaskExt,
    task::{ChildTaskFinished, ChildTaskSpawned, TaskStatus, TaskWorker},
};

/// Plugin for [`ParallelNode`] and [`ParallelTask`].
pub struct ParallelNodePlugin;

impl Plugin for ParallelNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<ParallelNode>()
            .with_setup_observer(on_parallel_setup_hook)
            .with_child_finish_observer(on_parallel_child_task_finished_hook)
            .with_child_spawned_observer(on_parallel_child_task_spawned_hook)
            .register();
    }
}

/// Policy for running a [`ParallelNode`].
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub enum ParallelPolicy {
    /// Run *ParallelNode* until a certain amount of children return required
    /// status.
    Fixed(usize),
    /// Run *ParallelNode* until all children return required status.
    #[default]
    All,
}

impl ParallelPolicy {
    /// Checks if `conter` reached a required amount of children for this
    /// policy.
    pub fn is_finished(&self, info: &CompositeNodeInfo, counter: usize) -> bool {
        match self {
            ParallelPolicy::Fixed(n) => counter >= *n,
            ParallelPolicy::All => counter >= info.len(),
        }
    }
}

/// How [`ParallelNode`] reacts to a child node finish before any other has been
/// spawned.
#[derive(Debug, Default, Reflect, Clone, Copy, PartialEq, Eq)]
pub enum ParallelEvaluation {
    /// Non-short-circuiting: every child spawns before *ParallelNode* reacts.
    #[default]
    Strict,
    /// Short-circuiting: a child finishing during setup can finish the node
    /// early.
    Lazy,
}

/// Node that will run all of the children in parallel until a certain
/// amount of them succeed or fails.
///
/// *ParallelNode* despite its name, does not use any special async
/// parallelization, it just starts all of its children at once and waits until
/// either the rules set for this node are met or all of the children are
/// event. In the first case *ParallelNode* will return [`TaskStatus::Success`],
/// and in the second one -- [`TaskStatus::Failure`].
///
/// Rules for this node are simple: a certain amount of successes/failures or
/// all of the child nodes must succeed or fail.
#[derive(Debug, Default, Reflect, Clone, Copy)]
pub struct ParallelNode {
    /// Policy for [`TaskStatus::Success`] of children nodes.
    pub success: ParallelPolicy,
    /// Policy for [`TaskStatus::Failure`] of children nodes.
    pub failure: ParallelPolicy,
    /// Whether children nodes can be reacted to before every one of them
    /// spawns.
    pub evaluation: ParallelEvaluation,
}

impl BehaviourNode for ParallelNode {
    type Info<'a> = CompositeNodeInfo<'a>;
    type Task = ParallelTask;

    fn build_task(&self) -> Self::Task { Self::Task::default() }
}

impl ParallelNode {
    /// Run this node until at least one of the children return
    /// [`TaskStatus::Success`].
    pub fn any_succeed() -> Self {
        Self {
            success: ParallelPolicy::Fixed(1),
            failure: ParallelPolicy::Fixed(0),
            evaluation: ParallelEvaluation::default(),
        }
    }

    /// Run this node until all of the children return [`TaskStatus::Success`].
    pub fn all_succeed() -> Self {
        Self {
            success: ParallelPolicy::All,
            failure: ParallelPolicy::Fixed(0),
            evaluation: ParallelEvaluation::default(),
        }
    }

    /// Run this node until at least one of the children return
    /// [`TaskStatus::Failure`].
    pub fn any_failed() -> Self {
        Self {
            success: ParallelPolicy::Fixed(0),
            failure: ParallelPolicy::Fixed(1),
            evaluation: ParallelEvaluation::default(),
        }
    }

    /// Run this node until all of the children return [`TaskStatus::Failure`].
    pub fn all_failed() -> Self {
        Self {
            success: ParallelPolicy::Fixed(0),
            failure: ParallelPolicy::All,
            evaluation: ParallelEvaluation::default(),
        }
    }

    /// Switch this node to [`ParallelEvaluation::Lazy`].
    pub fn lazy(mut self) -> Self {
        self.evaluation = ParallelEvaluation::Lazy;
        self
    }

    /// Switch this node to [`ParallelEvaluation::Strict`].
    pub fn strict(mut self) -> Self {
        self.evaluation = ParallelEvaluation::Strict;
        self
    }
}

/// Task for [`ParallelNode`].
#[derive(Default, Reflect)]
pub struct ParallelTask {
    /// Counter for succeeded children.
    pub success_counter: usize,
    /// Counter for failed children.
    pub failure_counter: usize,
    /// Children not yet spawned when evaluation is
    /// [`ParallelEvaluation::Strict`].
    strict_remaining: Option<usize>,
}

fn on_parallel_setup_hook(
    event: On<Add, TaskWorker<ParallelNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<ParallelNode>, NodeRef<ParallelNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.entity) else {
        return;
    };

    if node.info().is_empty() {
        cmd.entity(event.entity).insert(TaskStatus::Success);
        return;
    }

    if node.evaluation == ParallelEvaluation::Strict {
        task.strict_remaining = Some(node.info().len());
    }

    cmd.entity(event.entity).spawn_task(node.info().iter().collect::<Vec<_>>().as_slice());
}

fn on_parallel_child_task_spawned_hook(
    event: On<ChildTaskSpawned, TaskWorker<ParallelNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<ParallelNode>, NodeRef<ParallelNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.parent) else {
        return;
    };

    let Some(remaining) = task.strict_remaining.as_mut() else {
        return;
    };
    *remaining = remaining.saturating_sub(1);
    if *remaining > 0 {
        return;
    }

    if node.success.is_finished(&node.info(), task.success_counter)
        && node.failure.is_finished(&node.info(), task.failure_counter)
    {
        cmd.entity(event.parent).insert(TaskStatus::Success);
        return;
    }

    if task.success_counter + task.failure_counter >= node.info().len() {
        cmd.entity(event.parent).insert(TaskStatus::Failure);
    }
}

fn on_parallel_child_task_finished_hook(
    event: On<ChildTaskFinished, TaskWorker<ParallelNode>>,
    mut cmd: Commands,
    mut q_task: Query<(TaskMut<ParallelNode>, NodeRef<ParallelNode>)>,
) {
    let Ok((mut task, node)) = q_task.get_mut(event.task) else {
        return;
    };

    match event.child_status {
        TaskStatus::Success => {
            task.success_counter += 1;
        },
        TaskStatus::Failure => {
            task.failure_counter += 1;
        },
        TaskStatus::Running => {
            error!("Unexpected state of ParallelNode");
        },
    };

    if task.strict_remaining.is_some_and(|remaining| remaining > 0) {
        return;
    }

    if node.success.is_finished(&node.info(), task.success_counter)
        && node.failure.is_finished(&node.info(), task.failure_counter)
    {
        cmd.entity(event.task).insert(TaskStatus::Success);
        return;
    }

    if task.success_counter + task.failure_counter >= node.info().len() {
        cmd.entity(event.task).insert(TaskStatus::Failure);
    }
}
