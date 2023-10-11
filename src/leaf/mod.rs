//! Common leaf nodes (or action nodes, nodes without children).
//!
//! Provide nodes:
//! - [**WaitNode**][WaitNode]: Duration-based node. Can use bevy's [`Time`], or
//!   just count frames.
//! - [**SubtreeNode**][SubtreeNode]: Run other
//!   [`BehaviourTree`](crate::core::tree::BehaviourTree) tree as a part of this
//!   tree.
//! - [**LogNode**][LogNode]: Logs a message and return.
//! - [**ConstNode**][ConstNode]: Finishes immediately with a fixed status.

pub mod const_node;
pub mod log;
pub mod subtree;
pub mod wait;

use bevy::prelude::*;
pub use const_node::ConstNode;
pub use log::{LogLevel, LogNode};
pub use subtree::SubtreeNode;
pub use wait::{WaitMode, WaitNode, WaitTask};

/// Plugin for common leaf nodes.
pub struct LeafNodesPlugin;

impl Plugin for LeafNodesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            wait::WaitNodePlugin,
            subtree::SubtreeNodePlugin,
            log::LogNodePlugin,
            const_node::ConstNodePlugin,
        ));
    }
}
