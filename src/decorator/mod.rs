//! Common decorator nodes (with only one wrapped child node).
//
//! Provide nodes:
//! - [**LoopNode**][LoopNode]: Infinite loop
//! - [**LoopUntilNode**][LoopUntilNode]: Loop until specified status is
//!   returned
//! - [**LoopForNode**][LoopForNode]: Loop a fixed number of iterations
//! - [**NotNode**][NotNode]: Reverse return status of its child

pub mod r#loop;
pub mod loop_for;
pub mod loop_until;
pub mod not;

use bevy::prelude::*;
pub use r#loop::LoopNode;
pub use loop_for::{LoopForNode, LoopForTask};
pub use loop_until::{LoopUntilNode, LoopUntilTask};
pub use not::NotNode;

/// Plugin for common decorator nodes.
pub struct DecoratorNodesPlugin;

impl Plugin for DecoratorNodesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            r#loop::LoopNodePlugin,
            loop_until::LoopUntilNodePlugin,
            loop_for::LoopForNodePlugin,
            not::NotNodePlugin,
        ));
    }
}
