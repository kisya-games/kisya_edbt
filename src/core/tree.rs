//! Tree-structured collection of [`BehaviourNodes`][BehaviourNode].
//!
//! You can get trees in two ways:
//! - Manualy creating it;
//! - From asset using an [`AssetServer`];
//!
//! Note that trees from assets are validated on load, while trees created
//! manually are not (but you can still validate them via
//! [`BehaviourTree::validate`]).
//!
//! # Manually creating
//!
//! There are two ways of creating a tree and adding nodes:
//! [`behaviour_tree!`](crate::behaviour_tree) DSL, or manually via
//! [`BehaviourTree::new`] and [`BehaviourTree::push_child`] using
//! [`NodeIds`][NodeId] to track parents yourself (needed when the number of
//! children isn't known at compile time).
//!
//! ```rust
//! # use bevy::prelude::*;
//! # use kisya_edbt::prelude::*;
//! // Using a DSL
//! let dsl_tree = behaviour_tree! {
//!     LoopNode => [SequenceNode => [WaitNode::time(2.0), LogNode::info("Miao")]]
//! };
//!
//! // Manually
//! let mut manual_tree = BehaviourTree::new(LoopNode);
//! let seq = manual_tree.push_child(manual_tree.get_root_id(), SequenceNode);
//! let _wait = manual_tree.push_child(seq, WaitNode::time(2.0));
//! let _log = manual_tree.push_child(seq, LogNode::info("Miao"));
//!
//! assert_eq!(dsl_tree, manual_tree);
//! ```
//!
//! # Creating from an asset
//!
//! Bevy's [`AssetServer`] can load [`BehaviourTree`] as an Asset.
//! It uses amazing capabilities of bevy reflect to deserialize nodes. Asset is
//! a [RON][ron] file with a simple tree-like structure:
//!
//! ```ron
//! (root: {
//!     "LoopNode": ()     
//!     "children": [{
//!         "SequenceNode": ()     
//!         "children": [
//!             {
//!                 "someapp::AttackNode": (damage: 50)
//!             },
//!             {
//!                 "WaitNode": (duration: (secs: 5, nanos: 0))
//!             },
//!         ]
//!     }]
//! })
//! ```
//!
//! [ron]: https://github.com/ron-rs/ron

use std::{
    any::{Any, TypeId},
    borrow::Cow,
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use bevy::{
    asset::{AssetLoader, LoadContext, io::Reader},
    prelude::*,
    reflect::{FromType, Reflect, ReflectFromReflect, TypePath, TypeRegistry, TypeRegistryArc},
};
use itertools::Itertools;
use serde::de::DeserializeSeed;
use thiserror::Error;

use crate::core::{
    node::{
        BehaviourNode, BehaviourNodeInfo, LeafNodeInfo, NodeId, NodeValidationError, TreeNodeId,
    },
    ser::BehaviourTreeDeserializer,
};

/// Id of a loaded [`BehaviourTree`] asset.
pub type BehaviourTreeId = AssetId<BehaviourTree>;

/// Plugin for [`BehaviourTree`] and [`BehaviourTreeAssetLoader`].
pub struct BehaviourTreePlugin;

impl Plugin for BehaviourTreePlugin {
    fn build(&self, app: &mut App) {
        app.init_asset::<BehaviourTree>().init_asset_loader::<BehaviourTreeAssetLoader>();
    }
}

/// Tree of type-erased [`BehaviourNodes`][BehaviourNode].
#[derive(Asset, Debug, TypePath, PartialEq)]
pub struct BehaviourTree {
    root_id: indextree::NodeId,
    arena: indextree::Arena<DynamicNode>,
}

impl Default for BehaviourTree {
    fn default() -> Self {
        #[derive(Reflect, Default)]
        struct StubNode;
        impl BehaviourNode for StubNode {
            type Info<'a> = LeafNodeInfo<'a>;
            type Task = ();

            fn build_task(&self) -> Self::Task {}
        }
        Self::new(StubNode)
    }
}

impl BehaviourTree {
    /// Create a new tree from raw components.
    pub(crate) fn from_raw(arena: indextree::Arena<DynamicNode>, root: indextree::NodeId) -> Self {
        Self { arena, root_id: root }
    }

    /// Create a new tree with `root` node.
    pub fn new(root: impl BehaviourNode) -> Self {
        let mut arena = indextree::Arena::default();
        let root = arena.new_node(DynamicNode::from_node(root));

        Self { arena, root_id: root }
    }

    /// Validates the whole tree and return diagnostics.
    pub fn validate(&self, registry: &TypeRegistry) -> Vec<TreeValidationItem> {
        let mut diagnostics = Vec::new();

        for node in self.arena.iter() {
            let Some(node_id) = self.arena.get_node_id(node) else {
                continue;
            };
            if node.is_removed() {
                continue;
            }
            let node = node.get();
            let Some(reflect_node) = registry.get_type_data::<ReflectDynamicNode>(node.type_id())
            else {
                continue;
            };

            match (reflect_node.validate_fn)(NodeId { id: node_id, type_id: node.type_id() }, self)
            {
                Ok(_) => continue,
                Err(error) => {
                    let mut path = node_id
                        .ancestors(&self.arena)
                        .map(|node_id| self.create_node_id(node_id))
                        .collect::<Vec<_>>();
                    path.reverse();
                    diagnostics.push(TreeValidationItem { path, error });
                },
            }
        }
        diagnostics
    }

    /// Create a new detached node in the tree.
    pub fn new_node(&mut self, node: impl BehaviourNode) -> NodeId {
        self.new_node_dyn(DynamicNode::from_node(node))
    }

    /// Create a new detached node from a [`DynamicNode`].
    pub fn new_node_dyn(&mut self, dynamic_node: DynamicNode) -> NodeId {
        let arena_child_id = self.arena.new_node(dynamic_node);
        self.create_node_id(arena_child_id)
    }

    /// Add a new child to `parent_id`.
    pub fn push_child(&mut self, parent_id: NodeId, node: impl BehaviourNode) -> NodeId {
        self.push_child_dyn(parent_id, DynamicNode::from_node(node))
    }

    /// Add a new child to `parent_id`.
    pub(crate) fn push_child_dyn(
        &mut self,
        parent_id: NodeId,
        dynamic_node: DynamicNode,
    ) -> NodeId {
        let child_id = self.new_node_dyn(dynamic_node);
        self.append(parent_id, child_id);
        child_id
    }

    /// Detach a node from its parent.
    pub fn detach(&mut self, node_id: NodeId) { node_id.id.detach(&mut self.arena); }

    /// Add a child to a parent.
    pub fn append(&mut self, parent_id: NodeId, child_id: NodeId) {
        parent_id.id.append(child_id.id, &mut self.arena);
    }

    /// Remove a node and detach all of its children.
    pub fn remove_and_detach_children(&mut self, node_id: NodeId) {
        let children: Vec<indextree::NodeId> = node_id.id.children(&self.arena).collect();
        for child in children {
            child.detach(&mut self.arena);
        }
        node_id.id.remove(&mut self.arena);
    }

    /// Get [`NodeId`] of root node.
    pub fn get_root_id(&self) -> NodeId { self.create_node_id(self.root_id) }

    /// Make `node_id` the root of the tree.
    pub(crate) fn set_root(&mut self, node_id: NodeId) { self.root_id = node_id.id; }

    /// Iterate through all of the nodes that are not children of the root node.
    pub fn iter_orphans<'a>(&'a self) -> impl Iterator<Item = NodeId> + 'a {
        self.arena
            .iter()
            .filter(|node| !node.is_removed())
            .filter_map(|node| self.arena.get_node_id(node))
            .filter(|&node_id| {
                node_id != self.root_id && node_id.ancestors(&self.arena).nth(1).is_none()
            })
            .map(|n| self.create_node_id(n))
    }

    /// Get [`NodeId`] of parent of `node_id`.
    pub fn get_parent_id(&self, node_id: NodeId) -> Option<NodeId> {
        node_id.id.ancestors(&self.arena).nth(1).map(|id| self.create_node_id(id))
    }

    /// Get [`NodeId`] of child node of `node_id` at `index`.
    pub fn get_child_id(&self, node_id: NodeId, index: usize) -> Option<NodeId> {
        node_id.id.children(&self.arena).nth(index).map(|id| self.create_node_id(id))
    }

    /// Iterate through children ids of `node_id`.
    pub fn iter_children_id<'a>(&'a self, node_id: NodeId) -> impl Iterator<Item = NodeId> + 'a {
        node_id.id.children(&self.arena).map(|id| self.create_node_id(id))
    }

    /// Iterate through all of `node_id` descendants in a depth-first order.
    pub fn iter_descendants_id<'a>(&'a self, node_id: NodeId) -> impl Iterator<Item = NodeId> + 'a {
        node_id.id.descendants(&self.arena).skip(1).map(|id| self.create_node_id(id))
    }

    /// Get type-erased root node.
    pub fn get_root(&self) -> &DynamicNode { self.get_node(self.get_root_id()).unwrap() }

    /// Get type-erased root node.
    pub fn get_root_mut(&mut self) -> &mut DynamicNode {
        self.get_node_mut(self.get_root_id()).unwrap()
    }

    /// Get type-erased parent node of `node_id`.
    pub fn get_parent(&self, node_id: NodeId) -> Option<&DynamicNode> {
        let id = self.get_parent_id(node_id)?;
        self.get_node(id)
    }

    /// Get type-erased child node of `node_id` at `index`.
    pub fn get_child(&self, node_id: NodeId, index: usize) -> Option<&DynamicNode> {
        let id = self.get_child_id(node_id, index)?;
        self.get_node(id)
    }

    /// Get type-erased node by its [`NodeId`].
    pub fn get_node(&self, node_id: NodeId) -> Option<&DynamicNode> {
        self.arena.get(node_id.id).map(|node| node.get())
    }

    /// Get type-erased node by its [`NodeId`].
    pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut DynamicNode> {
        self.arena.get_mut(node_id.id).map(|node| node.get_mut())
    }

    /// Get size of children of `node_id`.
    pub fn get_children_len(&self, node_id: NodeId) -> usize {
        node_id.id.children(&self.arena).count()
    }

    fn create_node_id(&self, node_id: indextree::NodeId) -> NodeId {
        NodeId {
            id: node_id,
            type_id: self
                .arena
                .get(node_id)
                .map(|dynamic| dynamic.get().type_id())
                .unwrap_or(TypeId::of::<()>()),
        }
    }

    /// Return hash of the tree structure. It includes parent/child
    /// relationships and type information of all nodes (including detached
    /// ones).
    pub fn structural_hash(&self) -> u64 {
        let mut tree_hasher = DefaultHasher::default();
        for node in self.arena.iter() {
            if node.is_removed() {
                continue;
            }
            let mut node_hasher = DefaultHasher::default();

            node.parent().hash(&mut node_hasher);
            node.type_id().hash(&mut node_hasher);

            tree_hasher.write_u64(node_hasher.finish());
        }
        tree_hasher.finish()
    }
}

/// A piece of validation information, gathered from
/// [`BehaviourTree::validate`].
#[derive(Debug, Clone)]
pub struct TreeValidationItem {
    /// A full path from the root node to the node that failed validation.
    pub path: Vec<NodeId>,
    /// An error that failed validation for the specific node.
    pub error: NodeValidationError,
}

impl TreeValidationItem {
    /// Format path to a human readable format and replace [`NodeIds`][NodeId]
    /// with the actual node names.
    pub fn format_path(&self, tree: &BehaviourTree) -> String {
        self.path
            .iter()
            .map(|node_id| {
                tree.get_node(*node_id)
                    .map_or("Unknown node type", |dynamic_node| dynamic_node.name())
            })
            .enumerate()
            .fold(String::new(), |mut str, (i, name)| {
                str.push_str(&format!(
                    "{}{}{name}\n",
                    "  ".repeat(i),
                    if i == 0 { "" } else { "╰╴" }
                ));
                str
            })
    }

    /// Format validation item as a human-readable one-liner.
    pub fn format_path_one_line(&self, tree: &BehaviourTree) -> String {
        self.path
            .iter()
            .map(|node_id| {
                tree.get_node(*node_id)
                    .map_or("Unknown node type", |dynamic_node| dynamic_node.name())
            })
            .join("→")
    }
}

/// Builds a [`BehaviourTree`] from a compact DSL instead of manually calling
/// [`BehaviourTree::new`]/[`BehaviourTree::push_child`]:
///
/// ```
/// # use kisya_edbt::prelude::*;
/// let tree = behaviour_tree! {
///     LoopNode => [SequenceNode => [WaitNode::time(1.0), LogNode::info("Miao")]]
/// };
/// ```
///
/// A node with children is always followed by `=>` right before them, e.g.
/// `LoopNode => [A]` or `ParallelNode::any_succeed() => [A, B]`.
#[macro_export]
macro_rules! behaviour_tree {
    (@push $tree:ident, $parent:ident; ) => {};
    (@push $tree:ident, $parent:ident; $node:expr => [ $($children:tt)* ] $(, $($rest:tt)*)?) => {
        {
            let child = $tree.push_child($parent, $node);
            $crate::behaviour_tree!(@push $tree, child; $($children)*);
        }
        $crate::behaviour_tree!(@push $tree, $parent; $($($rest)*)?);
    };
    (@push $tree:ident, $parent:ident; $node:expr $(, $($rest:tt)*)?) => {
        $tree.push_child($parent, $node);
        $crate::behaviour_tree!(@push $tree, $parent; $($($rest)*)?);
    };
    ($root:expr => [ $($children:tt)* ]) => {{
        let mut tree = $crate::core::tree::BehaviourTree::new($root);
        let root = tree.get_root_id();
        $crate::behaviour_tree!(@push tree, root; $($children)*);
        tree
    }};
    ($root:expr) => {
        $crate::core::tree::BehaviourTree::new($root)
    };
}

/// Errors for [`DynamicNode`].
#[derive(Error, Debug)]
pub(crate) enum DynamicNodeError {
    #[error("No represented type info found for node")]
    NoTypeInfo,
    #[error("No registration data found for type {0}")]
    NoRegistrationData(String),
    #[error("No ReflectFromReflect found for type {0}")]
    NoReflectFromReflect(String),
    #[error("FromReflect failed for type {0}")]
    FromReflectFailed(String),
    #[error("No ReflectDefault found for type {0}")]
    NoReflectDefault(String),
}

/// Immutable container for type-erased [`BehaviourNode`].
pub struct DynamicNode {
    erased: Box<dyn Reflect + 'static>,
}

impl DynamicNode {
    /// Create a new node dynamic node from raw parts. Should be used for a
    /// registered type, and this reflected type must have
    /// [`ReflectFromReflect`] (by using ``).
    pub(crate) fn from_raw(
        raw: &Box<dyn PartialReflect + 'static>,
        registry: &TypeRegistry,
    ) -> Result<Self, DynamicNodeError> {
        let type_info = raw.get_represented_type_info().ok_or(DynamicNodeError::NoTypeInfo)?;
        let type_name = type_info.type_path_table().short_path();

        let registration = registry
            .get(type_info.type_id())
            .ok_or_else(|| DynamicNodeError::NoRegistrationData(type_name.to_string()))?;
        let rfr = registration
            .data::<ReflectFromReflect>()
            .ok_or_else(|| DynamicNodeError::NoReflectFromReflect(type_name.to_string()))?;
        let erased = rfr
            .from_reflect(raw.as_ref())
            .ok_or_else(|| DynamicNodeError::FromReflectFailed(type_name.to_string()))?;
        Ok(Self { erased })
    }

    pub(crate) fn from_type_id(
        type_id: TypeId,
        registry: &TypeRegistry,
    ) -> Result<Self, DynamicNodeError> {
        let type_info =
            registry.get(type_id).map(|r| r.type_info()).ok_or(DynamicNodeError::NoTypeInfo)?;
        let type_name = type_info.type_path_table().short_path();

        let registration = registry
            .get(type_info.type_id())
            .ok_or_else(|| DynamicNodeError::NoRegistrationData(type_name.to_string()))?;
        let reflect_default = registration
            .data::<ReflectDefault>()
            .ok_or(DynamicNodeError::NoReflectDefault(type_name.to_string()))?;
        let erased = reflect_default.default();
        Ok(Self { erased })
    }

    /// Wraps a new node as type-erased.
    pub fn from_node<N: BehaviourNode>(node: N) -> Self { Self { erased: Box::new(node) } }

    /// Get [`TypeId`] of this node.
    pub fn type_id(&self) -> TypeId { self.erased.get_represented_type_info().unwrap().type_id() }

    /// Get full name of this node.
    pub fn name(&self) -> &str { self.erased.reflect_short_type_path() }

    /// Get short name of this node.
    pub fn short_name(&self) -> &str { self.erased.reflect_short_type_path() }

    /// Get ident of this node.
    pub fn ident(&self) -> &str {
        self.erased.get_represented_type_info().unwrap().ty().ident().unwrap()
    }

    /// Get reflection of this node.
    pub fn get(&self) -> &dyn Reflect { self.erased.as_ref() }

    /// Get reflection of this node.
    pub fn get_mut(&mut self) -> &mut dyn Reflect { self.erased.as_mut() }

    /// Try to downcast this node to a concrete type.
    pub fn downcast<N: BehaviourNode>(&self) -> Option<&N> { self.erased.downcast_ref::<N>() }
}

impl std::fmt::Debug for DynamicNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.name()) }
}

impl std::fmt::Display for DynamicNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.ident())
    }
}

impl PartialEq for DynamicNode {
    fn eq(&self, other: &Self) -> bool { self.type_id() == other.type_id() }
}

/// Reflection data for a typed node to run validation on it.
#[derive(Debug, Clone)]
pub struct ReflectDynamicNode {
    validate_fn: fn(node_id: NodeId, tree: &BehaviourTree) -> Result<(), NodeValidationError>,
    available_slots_fn: fn(node_id: NodeId, tree: &BehaviourTree) -> usize,
    slot_name_fn:
        fn(node_id: NodeId, index: usize, tree: &BehaviourTree) -> Option<Cow<'static, str>>,
}

impl ReflectDynamicNode {
    /// Get the number of available slots for this node.
    pub fn available_slots(&self, node_id: NodeId, tree: &BehaviourTree) -> usize {
        (self.available_slots_fn)(node_id, tree)
    }

    /// Get the optional name for the slot at `index`.
    pub fn slot_name(
        &self,
        node_id: NodeId,
        index: usize,
        tree: &BehaviourTree,
    ) -> Option<Cow<'static, str>> {
        (self.slot_name_fn)(node_id, index, tree)
    }
}

impl<N: BehaviourNode> FromType<N> for ReflectDynamicNode {
    fn from_type() -> Self {
        Self {
            validate_fn: validate_node::<N>,
            available_slots_fn: available_slots::<N>,
            slot_name_fn: slot_name::<N>,
        }
    }
}

/// Asset loader for [`BehaviourTree`].
#[derive(TypePath)]
pub struct BehaviourTreeAssetLoader {
    type_registry: TypeRegistryArc,
}

impl FromWorld for BehaviourTreeAssetLoader {
    fn from_world(world: &mut World) -> Self {
        let type_registry = world.resource::<AppTypeRegistry>().0.clone();
        Self { type_registry }
    }
}

impl BehaviourTreeAssetLoader {
    /// Main loading function.
    async fn load_async<'a, 'ctx>(
        &'a self,
        bytes: &'a [u8],
        _settings: &'a (),
        _load_context: &'a mut LoadContext<'ctx>,
    ) -> anyhow::Result<BehaviourTree> {
        let mut bytes_deserializer = ron::de::Deserializer::from_bytes(bytes)?;
        let bt_deserializer = BehaviourTreeDeserializer::new(&self.type_registry);
        let tree = bt_deserializer.deserialize(&mut bytes_deserializer)?;

        let mut diagnostics = tree.validate(&self.type_registry.read());
        if !diagnostics.is_empty() {
            let diagnostics = diagnostics
                .drain(..)
                .map(|item| format!("{}\nError: {}", item.format_path(&tree), item.error))
                .join("\n--------------------\n");

            anyhow::bail!("Some nodes in the tree are invalid:\n{diagnostics}");
        }

        Ok(tree)
    }
}

impl AssetLoader for BehaviourTreeAssetLoader {
    type Asset = BehaviourTree;
    type Error = anyhow::Error;
    type Settings = ();

    async fn load<'a>(
        &'a self,
        reader: &mut dyn Reader,
        settings: &Self::Settings,
        load_context: &mut LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        self.load_async(&bytes, settings, load_context).await
    }
}

fn validate_node<N: BehaviourNode>(
    node_id: NodeId,
    tree: &BehaviourTree,
) -> Result<(), NodeValidationError> {
    let info = N::Info::from_id_and_tree(TreeNodeId { node: node_id, tree: default() }, tree);
    info.validate()
}

fn available_slots<N: BehaviourNode>(node_id: NodeId, tree: &BehaviourTree) -> usize {
    let info = N::Info::from_id_and_tree(TreeNodeId { node: node_id, tree: default() }, tree);
    info.available_slots()
}

fn slot_name<N: BehaviourNode>(
    node_id: NodeId,
    index: usize,
    tree: &BehaviourTree,
) -> Option<Cow<'static, str>> {
    let info = N::Info::from_id_and_tree(TreeNodeId { node: node_id, tree: default() }, tree);
    info.slot_name(index)
}
