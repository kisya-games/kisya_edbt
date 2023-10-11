//! General set of systems and tools to make a functional behaviour tree.

use std::any::TypeId;

use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    platform::collections::HashSet,
    prelude::*,
};

use crate::core::{
    task::{DisabledTask, TaskPool},
    tree::BehaviourTree,
};

pub mod node;
pub mod query;
pub mod registrar;
pub mod runner;
pub mod ser;
pub mod spawn;
pub mod task;
pub mod tree;

/// Event-driven behaviour tree plugin.
///
/// This plugin adds everything needed to start creating
/// [`BehaviourNodes`](crate::core::node::BehaviourNode), then adding them to
/// some [`BehaviourTree`] which can be run using [`Behaviour`] component.
///
/// Implementation is loosely based on [2nd generation GameAIPro BT][gameaipro].
///
/// *TODO: [an event extension][extension]*.
///
/// [gameaipro]: http://www.gameaipro.com/GameAIPro/GameAIPro_Chapter06_The_Behavior_Tree_Starter_Kit.pdf
/// [extension]: https://cs.uns.edu.ar/~ragis/Agis%20et%20al.%20(2020)%20-%20An%20event-driven%20behavior%20trees%20extension%20to%20facilitate%20non-player%20multi-agent%20coordination%20in%20video%20games.pdf
pub struct CoreBehaviourPlugin;

impl Plugin for CoreBehaviourPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            runner::BehaviourRunnerPlugin,
            tree::BehaviourTreePlugin,
            task::TaskPlugin,
        ));

        app.init_resource::<BehaviourTreeNodeLibrary>();
    }
}

/// Component that will run a [`BehaviourTree`] for this actor.
///
/// Re-inserting [`Behaviour`] will result in clearing the old [`TaskPool`] and
/// starting a new one. Behaviour updates can be disabled with
/// [`DisabledBehaviour`] component.
#[derive(Component, Reflect)]
#[component(immutable)]
pub struct Behaviour {
    /// A handle to the tree that this actor will run.
    pub tree: Handle<BehaviourTree>,
}

/// Actor-level marker that pauses its [`Behaviour`].
///
/// While present, every task in the actor's [`TaskPool`] gets [`DisabledTask`]
/// and is skipped by update systems; removing it re-enables them.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Reflect)]
#[component(on_add = Self::on_add_hook, on_remove = Self::on_remove_hook)]
pub struct DisabledBehaviour;

impl DisabledBehaviour {
    fn on_add_hook(mut world: DeferredWorld, ctx: HookContext) {
        let Some(pool) = world.get::<TaskPool>(ctx.entity) else {
            return;
        };
        let tasks: Vec<Entity> = pool.iter().collect();
        let mut commands = world.commands();
        for task in tasks {
            commands.entity(task).try_insert(DisabledTask);
        }
    }

    fn on_remove_hook(mut world: DeferredWorld, ctx: HookContext) {
        let Some(pool) = world.get::<TaskPool>(ctx.entity) else {
            return;
        };
        let tasks: Vec<Entity> = pool.iter().collect();
        let mut commands = world.commands();
        for task in tasks {
            commands.entity(task).try_remove::<DisabledTask>();
        }
    }
}

/// Global registry of available behaviour node types.
///
/// - Stored as `TypeId`s to avoid linking generic node types directly here.
/// - Populated by node registration; see [`registrar`] module.
#[derive(Resource, Default)]
pub struct BehaviourTreeNodeLibrary(HashSet<TypeId>);

impl BehaviourTreeNodeLibrary {
    /// Insert a node `TypeId` into the library. Returns whether it was newly
    /// inserted.
    pub(crate) fn insert_type_id(&mut self, type_id: TypeId) -> bool { self.0.insert(type_id) }

    /// Check if a node `TypeId` exists in the library.
    pub fn contains(&self, type_id: &TypeId) -> bool { self.0.contains(type_id) }

    /// Iterate over all registered node `TypeId`s.
    pub fn iter(&self) -> impl Iterator<Item = &TypeId> + '_ { self.0.iter() }
}

// TODO: someday...
// #[derive(Component, Default)]
// pub struct BehaviourBlackboard {
//     pub data: HashMap<String, Box<dyn Reflect>>,
// }
