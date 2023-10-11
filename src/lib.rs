//! # Behaviour Tree
//!
//! [`BehaviourTrees`](crate::core::tree::BehaviourTree) are a way to describe a
//! behaviour using small reusable blocks. Those blocks are
//! [`BehaviourNodes`](crate::core::node::BehaviourNode), and they can have
//! their own data and most importantly they are built using the usual bevy
//! systems and observers.
//!
//! Internaly trees store nodes in a type-erased structure (as `Reflect`). So
//! when working with tree a cheap [`NodeId`](crate::core::node::NodeId) (or
//! [`TreeNodeId`](crate::core::node::TreeNodeId), which stores both node and
//! tree parts) is used as the
//! smallest atomic descriptor. Alternatively, you can get raw type erased
//! [`DynamicNode`](crate::core::tree::DynamicNode) to inpsect actual data
//! stored inside nodes.
//!
//! Behaviour trees are assets, and can be loaded using a general methods of
//! working with assets in bevy:
//!
//! ```
//! # use bevy::prelude::*;
//! # use kisya_edbt::prelude::*;
//! # fn system(mut asset_server: ResMut<AssetServer>) {
//! let bt: Handle<BehaviourTree> = asset_server.load("enemy.bt");
//! # }
//! ```
//!
//! You can also use [`behaviour_tree!`] macro to build a
//! [`BehaviourTree`](crate::core::tree::BehaviourTree)
//! manually. See more about how to load or build your own tree in
//! [`tree`](crate::core::tree) module.
//!
//! ## Using a behaviour tree
//!
//! To actually make an entity run a
//! [`BehaviourTree`](crate::core::tree::BehaviourTree), you need
//! to add a [`Behaviour`](crate::core::Behaviour) component to an entity. And
//! that's pretty much it: changes to this component are watched and adding it
//! will queue a root node of specified tree to be run.
//!
//! Note that if [`Behaviour's`](crate::core::Behaviour) tree finishes (it's
//! task pool become empty), it will automatically remove
//! [`Behaviour`](crate::core::Behaviour) component from the entity.
//!
//! ```
//! # use bevy::prelude::*;
//! # use kisya_edbt::prelude::*;
//! # fn system(mut asset_server: ResMut<AssetServer>, mut commands: Commands) {
//! let tree = asset_server.load("chonker.bt");
//! let chonker = commands.spawn(Behaviour { tree }).id();
//! # }
//! ```
//!
//! If you want to pause an entity with [`Behaviour`](crate::core::Behaviour),
//! you can use [`DisabledBehaviour`](crate::core::DisabledBehaviour) component.
//! You can even spawn entities with it, and it
//! will create tasks, but remove those from node update systems/observers.
//!
//! ## Nodes
//!
//! [`BehaviourNodes`](crate::core::node::BehaviourNode) are a central building
//! block of a [`BehaviourTree`](crate::core::tree::BehaviourTree). They are a
//! reusable immutable structures, which can spawn tasks and know about their
//! children nodes. For every node, there are behaviour systems and observer
//! hooks that implement node's logic.
//!
//! Nodes are a part of a tree, but their [tasks](crate::core::task) are not --
//! those are simple bevy entities. Essentially, nodes are like small
//! sub-assets, shared between all tasks, and those tasks are actual working
//! units, that drive behaviour of actors. Tasks are represented by several
//! components (that you probably shouldn't use in node's systems, but more on
//! that later):
//! - [**TaskWorker**](crate::core::task::TaskWorker): Typed container for the
//!   actual task;
//! - [**TaskStatus**](crate::core::task::TaskStatus): Task's current status, as
//!   an immutable component -- inserting a finish status is what finishes a
//!   task;
//! - [**TaskOf**](crate::core::task::TaskOf) /
//!   [**TaskPool**](crate::core::task::TaskPool): Relationship linking a task
//!   to the actor entity it's running for;
//! - [**TaskChildOf**](crate::core::task::TaskChildOf) /
//!   [**TaskChildren**](crate::core::task::TaskChildren): Relationship pair
//!   linking a task to the task that spawned it;
//!
//! If you're using a [`Behaviour`](crate::core::Behaviour) component,
//! [`behaviour runner`](crate::core::runner) will take care of all of it.
//!
//! ### Behaviour systems and observers
//!
//! A node's task can be split in the following general phases:
//! - **Setup**: an `On<Add, TaskWorker<N>>` observer, run once when the task
//!   spawns;
//! - **Update**: a system run every frame while the task is running, usually
//!   with [`TaskMut<N>`](crate::core::query::TaskMut) /
//!   [`TaskRef<N>`](crate::core::query::TaskRef) in its queries;
//! - **Finish**: an `On<TaskFinished, TaskWorker<N>>` observer, run once when
//!   the task finishes, right before it despawns;
//! - **Children finish**: an `On<ChildTaskFinished, TaskWorker<N>>` observer,
//!   run every time this node's children are finished. This doesn't necessarily
//!   mean in-tree child nodes, since a node can spawn a task from a different
//!   tree.
//!
//! Update systems and observer hooks should prefer
//! [`TaskMut<N>`](crate::core::query::TaskMut) (or
//! [`TaskRef<N>`](crate::core::query::TaskRef) for read-only), which derefs to
//! the task itself and exposes its entity, actor and source node in one go,
//! while automatically skipping sleeping tasks. For node info (shared immutable
//! data or children of this node), [`NodeRef<N>`](crate::core::node::NodeRef)
//! is itself queriable, so it can just be added to the same query too.
//!
//! You are free to query whatever you want directly, but remember a task can
//! be *sleeping* (in multi-cycle environment, which can be used by
//! [`runner`](crate::core::runner) to run fresh batches of tasks within a
//! single update) -- filter it out with `Without<SleepingTask>`, or use
//! [`TaskMut`](crate::core::query::TaskMut)/
//! [`TaskRef`](crate::core::query::TaskRef)). Also be aware of *disabled*
//! tasks: [`DisabledTask`](crate::core::task::DisabledTask) marker, skipped
//! by any query that doesn't mention it, exactly like bevy's `Disabled`).
//!
//! ```
//! # use kisya_edbt::prelude::*;
//! # use bevy::prelude::*;
//! #[derive(Reflect, Default)]
//! struct TalkNode {
//!     msg: String,
//!     words_per_say: usize,
//! };
//! #[derive(Reflect)]
//! struct TalkTask {
//!     spoken_times: usize,
//! }
//!
//! impl BehaviourNode for TalkNode {
//!     type Info<'a> = LeafNodeInfo<'a>;
//!     type Task = TalkTask;
//!
//!     fn build_task(&self) -> Self::Task { Self::Task { spoken_times: 0 } }
//! }
//!
//! fn talk_node_system(
//!     mut cmd: Commands,
//!     mut q_task: Query<(TaskMut<TalkNode>, NodeRef<TalkNode>)>,
//! ) {
//!     for (task, node) in &mut q_task {
//!         let mut sentence = String::new();
//!         let mut words_iter = node
//!             .msg
//!             .split_whitespace()
//!             .skip(node.words_per_say * task.spoken_times)
//!             .take(node.words_per_say);
//!
//!         loop {
//!             match words_iter.next() {
//!                 Some(word) => {
//!                     sentence.push_str(word);
//!                     sentence.push(' ');
//!                 },
//!                 None => {
//!                     // Let the system know that this task is done and there are no errors.
//!                     cmd.entity(task.entity()).insert(TaskStatus::Success);
//!                     break;
//!                 },
//!             }
//!         }
//!
//!         info!("I say: {sentence} !");
//!     }
//! }
//!
//! // Run once when the task finishes, even if its source finished it.
//! fn on_talk_finished_hook(event: On<TaskFinished, TaskWorker<TalkNode>>) {
//!     info!("And I'm done talking !");
//! }
//! ```
//!
//! You can see more about how to use custom system queries and params in the
//! [`query`](crate::core::query) module.
//!
//! ### Registration
//!
//! Registration is done via
//! [`app.add_behaviour_node::<N>()`](crate::core::registrar::BehaviourNodeRegistrarAppExt::add_behaviour_node).
//!
//! It checks if a behaviour node is configured correctly (e.g. has at least one
//! behaviour system/observer), adds systems to internal schedulers, adds typed
//! resources required for running this node's tasks.
//!
//! ```
//! # use kisya_edbt::prelude::*;
//! # use bevy::prelude::*;
//! # #[derive(Reflect, Default)]
//! # struct CoolNode;
//! # impl BehaviourNode for CoolNode {
//! #     type Info<'a> = LeafNodeInfo<'a>;
//! #     type Task = ();
//! #
//! #     fn build_task(&self) -> Self::Task { () }
//! # }
//! # fn cool_node_system() {}
//! # fn on_cool_child_task_finished_hook(_: On<ChildTaskFinished, TaskWorker<CoolNode>>) {}
//! pub struct CoolNodePlugin;
//!
//! impl Plugin for CoolNodePlugin {
//!     fn build(&self, app: &mut App) {
//!         app.add_behaviour_node::<CoolNode>()
//!             .with_system(cool_node_system)
//!             .with_child_finish_observer(on_cool_child_task_finished_hook)
//!             .register();
//!     }
//! }
//! ```

#![warn(missing_docs)]

pub mod composite;
pub mod core;
pub mod decorator;
pub mod leaf;

#[cfg(feature = "egui")]
pub mod egui;

use bevy::app::{PluginGroup, PluginGroupBuilder};

/// Common structs and traits exported.
pub mod prelude {
    pub use crate::{
        BehaviourPlugins, behaviour_tree,
        composite::*,
        core::{
            Behaviour, BehaviourTreeNodeLibrary, DisabledBehaviour,
            node::{
                BehaviourNode, BehaviourNodeInfo, CompositeNodeInfo, DecoratorNodeInfo,
                LeafNodeInfo, NodeId, NodeRef,
            },
            query::{TaskMut, TaskRef},
            registrar::{BehaviourNodeAppRegistrar, BehaviourNodeRegistrarAppExt},
            ser::{BehaviourTreeDeserializer, BehaviourTreeSerializer},
            spawn::SpawnTaskExt,
            task::{
                ChildTaskFinished, ChildTaskSpawned, TaskFinished, TaskPool, TaskStatus, TaskWorker,
            },
            tree::BehaviourTree,
        },
        decorator::*,
        leaf::*,
    };
}

/// Full plugin set for Behaviour Trees.
///
/// - [CoreBehaviourPlugin](core::CoreBehaviourPlugin): necessary systems and
///   components to run any Behaviour Tree;
/// - [CompositeNodesPlugin](composite::CompositeNodesPlugin): introduces common
///   composite nodes;
/// - [DecoratorNodesPlugin](decorator::DecoratorNodesPlugin): introduces common
///   decorator nodes;
/// - [LeafNodesPlugin](leaf::LeafNodesPlugin): introduces common leaf nodes;
/// - [BehaviourTreeEguiEditorPlugin](egui::BehaviourTreeEguiEditorPlugin):
///   configures egui to enable editor (optional, gated by `egui` feature).
pub struct BehaviourPlugins;

impl PluginGroup for BehaviourPlugins {
    fn build(self) -> PluginGroupBuilder {
        let builder = PluginGroupBuilder::start::<Self>()
            .add(core::CoreBehaviourPlugin)
            .add(composite::CompositeNodesPlugin)
            .add(decorator::DecoratorNodesPlugin)
            .add(leaf::LeafNodesPlugin);

        #[cfg(feature = "egui")]
        let builder = builder.add(egui::BehaviourTreeEguiEditorPlugin);

        builder
    }
}
