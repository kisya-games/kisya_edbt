//! [`WaitNode`] and related structs.

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, LeafNodeInfo},
    query::TaskMut,
    registrar::BehaviourNodeRegistrarAppExt,
    task::TaskStatus,
};

/// Plugin for [`WaitNode`].
pub struct WaitNodePlugin;

impl Plugin for WaitNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<WaitNode>().with_system(wait_node_system).register();
    }
}

/// What a [`WaitNode`] waits on.
#[derive(Debug, Reflect, Clone, Copy, PartialEq)]
pub enum WaitMode {
    /// Wait a real-time duration, ticking seconds off a timer with
    /// [`Time::delta()`][Time::delta].
    Time(f32),
    /// Wait a fixed number of update frames regardless of time.
    Frames(u32),
}

/// Node that returns [`TaskStatus::Success`] once a wait elapses,
/// [`TaskStatus::Running`] until then.
#[derive(Debug, Reflect, Clone, Copy)]
pub struct WaitNode {
    /// Mode in which this node will wait.
    pub mode: WaitMode,
}

impl WaitNode {
    /// Create a node that waits `duration` seconds.
    pub fn time(duration: f32) -> Self { Self { mode: WaitMode::Time(duration) } }

    /// Create a node that waits `frames` update frames, ignoring time.
    pub fn frames(frames: u32) -> Self { Self { mode: WaitMode::Frames(frames) } }
}

impl Default for WaitNode {
    fn default() -> Self { Self::frames(1) }
}

impl BehaviourNode for WaitNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = WaitTask;

    fn build_task(&self) -> Self::Task {
        match self.mode {
            WaitMode::Time(duration) => {
                WaitTask::Time(Timer::from_seconds(duration, TimerMode::Once))
            },
            WaitMode::Frames(frames) => WaitTask::Frames(frames),
        }
    }
}

/// Task for [`WaitNode`].
#[derive(Reflect, Debug)]
pub enum WaitTask {
    /// Timer counting down real time for [`WaitMode::Time`].
    Time(Timer),
    /// Frames left to wait for [`WaitMode::Frames`].
    Frames(u32),
}

impl Default for WaitTask {
    fn default() -> Self { Self::Time(Timer::default()) }
}

/// System to update [`WaitNode`].
fn wait_node_system(mut cmd: Commands, time: Res<Time>, mut q_task: Query<TaskMut<WaitNode>>) {
    let delta = time.delta();
    for mut task in &mut q_task {
        let entity = task.entity();
        let finished = match &mut *task {
            WaitTask::Time(timer) => {
                timer.tick(delta);
                timer.is_finished()
            },
            WaitTask::Frames(remaining) => {
                *remaining = remaining.saturating_sub(1);
                *remaining == 0
            },
        };
        if finished {
            cmd.entity(entity).insert(TaskStatus::Success);
        }
    }
}
