//! [`LogNode`] and related structs.

use std::borrow::Cow;

use bevy::prelude::*;

use crate::core::{
    node::{BehaviourNode, LeafNodeInfo, NodeRef},
    registrar::BehaviourNodeRegistrarAppExt,
    task::{TaskStatus, TaskWorker},
};

/// Plugin for [`LogNode`].
pub struct LogNodePlugin;

impl Plugin for LogNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<LogNode>().with_setup_observer(on_log_setup_hook).register();
    }
}

/// Log level for [`LogNode`]. It mirrors [bevy's
/// `Level`](bevy::log::Level), because bevy's version doesn't have
/// reflect.
#[derive(Debug, Reflect, Clone, Copy, PartialEq, Eq, Default)]
pub enum LogLevel {
    /// Logs with [`trace!`].
    Trace,
    /// Logs with [`debug!`].
    #[default]
    Debug,
    /// Logs with [`info!`].
    Info,
    /// Logs with [`warn!`].
    Warn,
    /// Logs with [`error!`].
    Error,
}

/// Node that will log a message and return [`TaskStatus::Success`].
#[derive(Debug, Reflect, Clone, Default)]
pub struct LogNode {
    /// Message to be logged.
    pub message: Cow<'static, str>,
    /// Measage's log level.
    pub level: LogLevel,
}

impl LogNode {
    /// Create a new trace message node.
    pub fn trace(message: impl Into<Cow<'static, str>>) -> Self {
        Self { message: message.into(), level: LogLevel::Trace }
    }

    /// Create a new debug message node.
    pub fn debug(message: impl Into<Cow<'static, str>>) -> Self {
        Self { message: message.into(), level: LogLevel::Debug }
    }

    /// Create a new info message node.
    pub fn info(message: impl Into<Cow<'static, str>>) -> Self {
        Self { message: message.into(), level: LogLevel::Info }
    }

    /// Create a new warn message node.
    pub fn warn(message: impl Into<Cow<'static, str>>) -> Self {
        Self { message: message.into(), level: LogLevel::Warn }
    }

    /// Create a new error message node.
    pub fn error(message: impl Into<Cow<'static, str>>) -> Self {
        Self { message: message.into(), level: LogLevel::Error }
    }
}

impl BehaviourNode for LogNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ();

    fn build_task(&self) -> Self::Task { () }
}

fn on_log_setup_hook(
    event: On<Add, TaskWorker<LogNode>>,
    mut cmd: Commands,
    q_task: Query<NodeRef<LogNode>>,
) {
    let Ok(node) = q_task.get(event.entity) else {
        return;
    };
    let msg = node.message.as_ref();

    match node.level {
        LogLevel::Trace => {
            trace!("{msg}");
        },
        LogLevel::Debug => {
            debug!("{msg}");
        },
        LogLevel::Info => {
            info!("{msg}");
        },
        LogLevel::Warn => {
            warn!("{msg}");
        },
        LogLevel::Error => {
            error!("{msg}");
        },
    }

    cmd.entity(event.entity).insert(TaskStatus::Success);
}
