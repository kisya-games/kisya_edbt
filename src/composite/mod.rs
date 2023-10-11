//! Common composite nodes (with multiple child nodes).
//!
//! Provide nodes:
//! - [**ConditionNode**][ConditionNode]: If the first child succeded, then run
//!   the second child, otherwise run the optional third child.
//! - [**OneOfNode**][OneOfNode]: Out of number of children, pick (randomly) and
//!   run only one. Requires the `random` feature.
//! - [**ParallelNode**][ParallelNode]: Run every children at once until number
//!   of them is succeded or failed.
//! - [**SequenceNode**][SequenceNode]: Run children nodes one by one until one
//!   is failed.
//! - [**WhileNode**][WhileNode]: Run two children at once, and keep doing that,
//!   while the first one keep succeding.

pub mod condition;
#[cfg(feature = "random")]
pub mod one_of;
pub mod parallel;
pub mod sequence;
pub mod r#while;

use bevy::prelude::*;
pub use condition::{ConditionNode, ConditionTask};
#[cfg(feature = "random")]
pub use one_of::OneOfNode;
pub use parallel::{ParallelNode, ParallelTask};
pub use sequence::{SequenceNode, SequenceTask};
pub use r#while::{WhileNode, WhileTask};

/// Plugin for common composite nodes.
pub struct CompositeNodesPlugin;

impl Plugin for CompositeNodesPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            sequence::SequenceNodePlugin,
            condition::ConditionNodePlugin,
            parallel::ParallelNodePlugin,
            r#while::WhileNodePlugin,
        ));

        #[cfg(feature = "random")]
        app.add_plugins(one_of::OneOfNodePlugin);
    }
}
