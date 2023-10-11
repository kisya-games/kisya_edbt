//! [`BehaviourNode`] and its related structs.

use std::{any::TypeId, borrow::Cow, ops::Deref};

use bevy::{prelude::*, reflect::Reflectable};

use crate::core::tree::{BehaviourTree, BehaviourTreeId};

/// Errors for [`BehaviourNodeInfo::validate`].
#[derive(thiserror::Error, Debug, Clone)]
pub enum NodeValidationError {
    /// Node has more children that it should.
    #[error("Node has too much children: {0} (maximum is {1})")]
    TooMuchChildren(usize, usize),
    /// Node has fewer children than it should.
    #[error("Node has too few children: {0} (minimum is {1})")]
    TooFewChildren(usize, usize),
    /// Node has no children while it should have some.
    #[error("Node is not supposed to be empty")]
    IsEmpty,
    /// Node is not connected to root.
    #[error("Node is not connected to root")]
    NotConnectedToRoot,
    /// Node has invalid child count.
    #[error("Node has invalid child count: {0} (expected {1})")]
    InvalidChildCount(usize, usize),
    /// Custom error message.
    #[error("{0}")]
    Custom(String),
}

/// Base interface for custom logic in a [`BehaviourTree`].
///
/// Node serves a purpuse of an entry point for every dynamic
/// action in a [`BehaviourTree`], with ability to spawn
/// mutable tasks.
///
/// In general, global immutable data required for running this
/// node should be stored inside the node itself, while everything
/// else can be stored inside node's task per instance.
pub trait BehaviourNode: Reflectable + Default + Send + Sync + 'static {
    /// Per instance data required for running this node.
    type Task: Reflectable + Send + Sync;
    /// Node's children access. Can be one of [`CompositeNodeInfo`],
    /// [`DecoratorNodeInfo`], [`LeafNodeInfo`] or your own.
    type Info<'a>: BehaviourNodeInfo<'a>;

    /// Create a new task of this node.
    fn build_task(&self) -> Self::Task;
}

/// Lightweight handle to a [`BehaviourNode`].
#[derive(Reflect, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[reflect(opaque)]
pub struct NodeId {
    pub(crate) id: indextree::NodeId,
    pub(crate) type_id: TypeId,
}

/// Self-sufficient handle to a [`BehaviourNode`], resolvable to a
/// [`BehaviourTree`] on its own (unlike [`NodeId`], which is only meaningful
/// alongside a [`BehaviourTree`] reference you already hold).
#[derive(Reflect, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreeNodeId {
    /// Id of the node within [`tree`][Self::tree].
    pub node: NodeId,
    /// Tree that [`node`][Self::node] belongs to.
    pub tree: BehaviourTreeId,
}

/// Lightweight wrapper of [`BehaviourNode`] with its associated
/// [`BehaviourTree`].
pub struct NodeRef<'a, N: BehaviourNode> {
    node: &'a N,
    id: TreeNodeId,
    tree: &'a BehaviourTree,
}

impl<N: BehaviourNode> Deref for NodeRef<'_, N> {
    type Target = N;

    fn deref(&self) -> &Self::Target { self.get() }
}

impl<N: BehaviourNode> Clone for NodeRef<'_, N> {
    fn clone(&self) -> Self { Self { node: self.node, id: self.id, tree: self.tree } }
}

impl<'a, N: BehaviourNode> NodeRef<'a, N> {
    /// Tries to create a new ref.
    pub fn try_new(id: TreeNodeId, tree: &'a BehaviourTree) -> Option<Self> {
        tree.get_node(id.node)
            .and_then(|dynamic_node| dynamic_node.downcast::<N>())
            .map(|node| Self { node, id, tree })
    }

    /// Get a node reference.
    pub fn get(&self) -> &N { self.node }

    /// Get a [`TreeNodeId`] of this node.
    pub fn id(&self) -> TreeNodeId { self.id }

    /// Get a [`BehaviourTree`] of this node.
    pub fn tree(&self) -> &BehaviourTree { self.tree }

    /// Get [`BehaviourNodeInfo`] of this node.
    pub fn info<'b>(&'b self) -> N::Info<'b> { N::Info::from_id_and_tree(self.id(), self.tree()) }
}

/// Descriptor of [`BehaviourNode's`][BehaviourNode] acces to children.
pub trait BehaviourNodeInfo<'a>: Copy {
    /// Create a new descriptor for [`TreeNodeId`] and [`BehaviourTree`].
    fn from_id_and_tree(id: TreeNodeId, tree: &'a BehaviourTree) -> Self;

    /// Validate that this node is correct considering it's children.
    fn validate(&self) -> Result<(), NodeValidationError>;

    /// Get the number of available slots for this node.
    fn available_slots(&self) -> usize;

    /// Optional name for the slot.
    fn slot_name(&self, _index: usize) -> Option<Cow<'static, str>> { None }
}

/// Children access descriptor for Composite nodes, which can have
/// zero or more children nodes.
#[derive(Clone, Copy)]
pub struct CompositeNodeInfo<'a> {
    id: TreeNodeId,
    tree: &'a BehaviourTree,
}

impl<'a> BehaviourNodeInfo<'a> for CompositeNodeInfo<'a> {
    fn from_id_and_tree(id: TreeNodeId, tree: &'a BehaviourTree) -> Self { Self { id, tree } }

    fn validate(&self) -> Result<(), NodeValidationError> {
        if self.is_empty() {
            return Err(NodeValidationError::IsEmpty);
        }

        Ok(())
    }

    fn available_slots(&self) -> usize { self.len() + 1 }

    fn slot_name(&self, index: usize) -> Option<Cow<'static, str>> {
        if index < self.available_slots() { Some(Cow::Borrowed("Child")) } else { None }
    }
}

impl CompositeNodeInfo<'_> {
    /// Return size of available children nodes.
    pub fn len(&self) -> usize { self.tree.get_children_len(self.id.node) }

    /// Try to get a child node at `index`.
    pub fn get_child(&self, index: usize) -> Option<TreeNodeId> {
        self.tree
            .get_child_id(self.id.node, index)
            .map(|node| TreeNodeId { node, tree: self.id.tree })
    }

    /// Return iterator with all available children nodes.
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = TreeNodeId> + 'a {
        self.tree.iter_children_id(self.id.node).map(|node| TreeNodeId { node, tree: self.id.tree })
    }

    /// Check if there are any children nodes or not.
    pub fn is_empty(&self) -> bool { self.len() == 0 }
}

/// Children access descriptor for Decorator nodes, which are
/// basically wrappers of some other single node.
#[derive(Clone, Copy)]
pub struct DecoratorNodeInfo<'a> {
    id: TreeNodeId,
    tree: &'a BehaviourTree,
}

impl<'a> BehaviourNodeInfo<'a> for DecoratorNodeInfo<'a> {
    fn from_id_and_tree(id: TreeNodeId, tree: &'a BehaviourTree) -> Self { Self { id, tree } }

    fn validate(&self) -> Result<(), NodeValidationError> {
        let children = self.tree.get_children_len(self.id.node);
        if children > 1 {
            return Err(NodeValidationError::TooMuchChildren(children, 1));
        } else if children == 0 {
            return Err(NodeValidationError::IsEmpty);
        }

        Ok(())
    }

    fn available_slots(&self) -> usize { 1 }

    fn slot_name(&self, index: usize) -> Option<Cow<'static, str>> {
        if index == 0 { Some(Cow::Borrowed("Inner")) } else { None }
    }
}

impl DecoratorNodeInfo<'_> {
    /// Get a wrapped child node.
    pub fn get_child(&self) -> Option<TreeNodeId> {
        self.tree
            .iter_children_id(self.id.node)
            .next()
            .map(|node| TreeNodeId { node, tree: self.id.tree })
    }

    /// Check if there is a wrapped child node.
    pub fn is_empty(&self) -> bool { self.get_child().is_none() }
}

/// Children access descriptor for Leaf nodes, which have no
/// children nodes.
#[derive(Clone, Copy)]
pub struct LeafNodeInfo<'a> {
    id: TreeNodeId,
    tree: &'a BehaviourTree,
}

impl<'a> BehaviourNodeInfo<'a> for LeafNodeInfo<'a> {
    fn from_id_and_tree(id: TreeNodeId, tree: &'a BehaviourTree) -> Self { Self { id, tree } }

    fn validate(&self) -> Result<(), NodeValidationError> {
        let children = self.tree.get_children_len(self.id.node);
        if children > 0 {
            return Err(NodeValidationError::TooMuchChildren(children, 0));
        }

        Ok(())
    }

    fn available_slots(&self) -> usize { 0 }
}
