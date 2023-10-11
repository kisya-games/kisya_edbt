//! [`BehaviourTree`] serialization and deserialization utilities.

use bevy::{
    prelude::*,
    reflect::{
        PartialReflect, TypeRegistry, TypeRegistryArc,
        serde::{TypeRegistrationDeserializer, TypedReflectDeserializer, TypedReflectSerializer},
    },
};
use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    ser::{self, SerializeMap, SerializeSeq, SerializeStruct},
};

use crate::core::{
    node::NodeId,
    tree::{BehaviourTree, DynamicNode},
};

const TREE_STRUCT: &str = "Tree";
const TREE_ROOT: &str = "root";

const NODE_CHILDREN: &str = "children";

/// Serializer for whole [`BehaviourTree`].
/// It uses bevy's [`Reflect`] and App's [`TypeRegistryArc`].
pub struct BehaviourTreeSerializer<'tree, 'reg> {
    tree: &'tree BehaviourTree,
    type_registry: &'reg TypeRegistryArc,
}

impl<'tree, 'reg> BehaviourTreeSerializer<'tree, 'reg> {
    /// Create a new [`BehaviourTree`] serializer.
    pub fn new(tree: &'tree BehaviourTree, type_registry: &'reg TypeRegistryArc) -> Self {
        Self { tree, type_registry }
    }
}

impl<'tree, 'reg> Serialize for BehaviourTreeSerializer<'tree, 'reg> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(TREE_STRUCT, 1)?;
        state.serialize_field(
            TREE_ROOT,
            &BehaviourTreeNodeSerializer::new(
                self.tree,
                self.type_registry,
                self.tree.get_root_id(),
            ),
        )?;

        state.end()
    }
}

/// Serializer for a single node in a [`BehaviourTree`]
/// It uses bevy's [`Reflect`] and App's [`TypeRegistryArc`].
struct BehaviourTreeNodeSerializer<'tree, 'reg> {
    tree: &'tree BehaviourTree,
    type_registry: &'reg TypeRegistryArc,
    node_id: NodeId,
}

impl<'tree, 'reg> BehaviourTreeNodeSerializer<'tree, 'reg> {
    /// Create a new [`BehaviourNode`](crate::core::node::BehaviourNode)
    /// serializer.
    fn new(
        tree: &'tree BehaviourTree,
        type_registry: &'reg TypeRegistryArc,
        node_id: NodeId,
    ) -> Self {
        Self { tree, type_registry, node_id }
    }
}

impl<'tree, 'reg> Serialize for BehaviourTreeNodeSerializer<'tree, 'reg> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_map(Some(2))?;
        let dynamic_node = self
            .tree
            .get_node(self.node_id)
            .ok_or_else(|| ser::Error::custom("Invalid NodeId used"))?;

        state.serialize_entry(
            dynamic_node.get().reflect_short_type_path(),
            &TypedReflectSerializer::new(dynamic_node.get(), &self.type_registry.read()),
        )?;
        state.serialize_entry(
            NODE_CHILDREN,
            &BehaviourTreeNodesSerializer::new(
                self.tree,
                self.type_registry,
                self.tree.iter_children_id(self.node_id),
            ),
        )?;

        state.end()
    }
}

/// Serializer for multiple nodes in a [`BehaviourTree`]
/// It uses bevy's [`Reflect`] and App's [`TypeRegistryArc`].
struct BehaviourTreeNodesSerializer<'tree, 'reg> {
    tree: &'tree BehaviourTree,
    type_registry: &'reg TypeRegistryArc,
    node_ids: Vec<NodeId>,
}

impl<'tree, 'reg> BehaviourTreeNodesSerializer<'tree, 'reg> {
    /// Create a serializer of sequence of
    /// [`BehaviourNode`](crate::core::node::BehaviourNode).
    fn new(
        tree: &'tree BehaviourTree,
        type_registry: &'reg TypeRegistryArc,
        node_ids: impl IntoIterator<Item = NodeId>,
    ) -> Self {
        Self { tree, type_registry, node_ids: node_ids.into_iter().collect() }
    }
}

impl<'tree, 'reg> Serialize for BehaviourTreeNodesSerializer<'tree, 'reg> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_seq(Some(self.node_ids.len()))?;
        for node_id in &self.node_ids {
            state.serialize_element(&BehaviourTreeNodeSerializer::new(
                self.tree,
                self.type_registry,
                *node_id,
            ))?;
        }

        state.end()
    }
}

/// Fields of [`BehaviourTreeDeserializer`].
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum BehaviourTreeField {
    Root,
}

/// Intermediate behaviour tree node representation used in deserialization.
struct BehaviourTreeNodeItem {
    node: Box<dyn PartialReflect>,
    children: Vec<BehaviourTreeNodeItem>,
}

impl BehaviourTreeNodeItem {
    /// Assume this node item as a root of a [`BehaviourTree`], and creates
    /// a tree out of it.
    fn create_tree(&self, type_registry: &TypeRegistry) -> Result<BehaviourTree> {
        let mut arena = indextree::Arena::default();
        let root = self.add_to_arena(&mut arena, None, type_registry)?;

        Ok(BehaviourTree::from_raw(arena, root))
    }

    /// Add this node item to a certain tree.
    fn add_to_arena(
        &self,
        arena: &mut indextree::Arena<DynamicNode>,
        parent_node: Option<indextree::NodeId>,
        type_registry: &TypeRegistry,
    ) -> Result<indextree::NodeId> {
        let dynamic_node = DynamicNode::from_raw(&self.node, type_registry)?;

        let id = arena.new_node(dynamic_node);
        if let Some(parent_id) = parent_node {
            parent_id.append(id, arena);
        }
        for item in &self.children {
            item.add_to_arena(arena, Some(id), type_registry)?;
        }

        Ok(id)
    }
}

/// Deserializer for whole [`BehaviourTree`].
/// It uses bevy's [`FromReflect`], and App's [`TypeRegistryArc`].
pub struct BehaviourTreeDeserializer<'reg> {
    type_registry: &'reg TypeRegistryArc,
}

impl<'reg> BehaviourTreeDeserializer<'reg> {
    /// Create a new deserializer.
    pub fn new(type_registry: &'reg TypeRegistryArc) -> Self { Self { type_registry } }
}

impl<'reg, 'de> DeserializeSeed<'de> for BehaviourTreeDeserializer<'reg> {
    type Value = BehaviourTree;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(TREE_STRUCT, &[TREE_ROOT], BehaviourTreeVisitor {
            type_registry: self.type_registry,
        })
    }
}

/// Visitor for [`BehaviourTreeDeserializer`].
struct BehaviourTreeVisitor<'a> {
    type_registry: &'a TypeRegistryArc,
}

impl<'a, 'de> Visitor<'de> for BehaviourTreeVisitor<'a> {
    type Value = BehaviourTree;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("behaviour tree struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let root_item = seq
            .next_element_seed(BehaviourTreeNodeDeserializer { type_registry: self.type_registry })?
            .ok_or_else(|| de::Error::missing_field(TREE_ROOT))?;

        root_item.create_tree(&self.type_registry.read()).map_err(|err| {
            de::Error::custom(format!(
                "Cannot create a behaviour tree from deserialized nodes: {err}"
            ))
        })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut root_item = None;

        while let Some(key) = map.next_key()? {
            match key {
                BehaviourTreeField::Root => {
                    if root_item.is_some() {
                        return Err(de::Error::duplicate_field(TREE_ROOT));
                    }

                    root_item = map
                        .next_value_seed(BehaviourTreeNodeDeserializer {
                            type_registry: self.type_registry,
                        })?
                        .into();
                },
            }
        }

        let root_item = root_item.take().ok_or_else(|| de::Error::missing_field(TREE_ROOT))?;

        root_item.create_tree(&self.type_registry.read()).map_err(|err| {
            de::Error::custom(format!(
                "Cannot create a behaviour tree from deserialized nodes: {err}"
            ))
        })
    }
}

/// Deserializer for a single
/// [`BehaviourNode`](crate::core::node::BehaviourNode). It uses bevy's
/// [`FromReflect`], and App's [`TypeRegistryArc`].
struct BehaviourTreeNodeDeserializer<'reg> {
    type_registry: &'reg TypeRegistryArc,
}

impl<'reg, 'de> DeserializeSeed<'de> for BehaviourTreeNodeDeserializer<'reg> {
    type Value = BehaviourTreeNodeItem;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(BehaviourTreeNodeVisitor { type_registry: self.type_registry })
    }
}

/// Visitor for [`BehaviourTreeNodeDeserializer`].
struct BehaviourTreeNodeVisitor<'reg> {
    type_registry: &'reg TypeRegistryArc,
}

impl<'reg, 'de> Visitor<'de> for BehaviourTreeNodeVisitor<'reg> {
    type Value = BehaviourTreeNodeItem;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("behaviour tree node struct")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let type_registry = self.type_registry.read();

        let registration = seq
            .next_element_seed(TypeRegistrationDeserializer::new(&type_registry))?
            .ok_or_else(|| de::Error::missing_field("node type"))?;

        let node = seq
            .next_element_seed(TypedReflectDeserializer::new(registration, &type_registry))?
            .ok_or_else(|| de::Error::missing_field("node data"))?;

        let children = seq
            .next_element_seed(BehaviourTreeNodesDeserializer {
                type_registry: &self.type_registry,
            })?
            .unwrap_or_default();

        Ok(BehaviourTreeNodeItem { node, children })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut children = None;
        let mut node = None;

        while let Some(key) = map.next_key::<&str>()? {
            match key {
                NODE_CHILDREN => {
                    if children.is_some() {
                        return Err(de::Error::duplicate_field(NODE_CHILDREN));
                    }
                    children = map
                        .next_value_seed(BehaviourTreeNodesDeserializer {
                            type_registry: &self.type_registry,
                        })?
                        .into();
                },
                type_name => {
                    if node.is_some() {
                        return Err(de::Error::duplicate_field("node"));
                    }
                    let type_registry = self.type_registry.read();
                    let registration = type_registry
                        .get_with_type_path(type_name)
                        .or_else(|| type_registry.get_with_short_type_path(type_name))
                        .ok_or_else(|| {
                            de::Error::custom(format!("Unregistered node type: {type_name}"))
                        })?;

                    node = map
                        .next_value_seed(TypedReflectDeserializer::new(
                            registration,
                            &type_registry,
                        ))?
                        .into()
                },
            }
        }

        let children = children.take().unwrap_or_default();
        let node = node.take().ok_or_else(|| de::Error::missing_field("node"))?;

        Ok(BehaviourTreeNodeItem { node, children })
    }
}

/// Deserializer for multiple
/// [`BehaviourNodes`](crate::core::node::BehaviourNode). It uses bevy's
/// [`FromReflect`], and App's [`TypeRegistryArc`].
struct BehaviourTreeNodesDeserializer<'reg> {
    type_registry: &'reg TypeRegistryArc,
}

impl<'reg, 'de> DeserializeSeed<'de> for BehaviourTreeNodesDeserializer<'reg> {
    type Value = Vec<BehaviourTreeNodeItem>;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer
            .deserialize_seq(BehaviourTreeNodesVisitor { type_registry: self.type_registry })
    }
}

/// Visitor for [`BehaviourTreeNodesDeserializer`].
struct BehaviourTreeNodesVisitor<'a> {
    type_registry: &'a TypeRegistryArc,
}

impl<'a, 'de> Visitor<'de> for BehaviourTreeNodesVisitor<'a> {
    type Value = Vec<BehaviourTreeNodeItem>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("behaviour tree nodes sequence")
    }

    fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut vec = Vec::new();
        while let Some(node) = seq.next_element_seed(BehaviourTreeNodeDeserializer {
            type_registry: self.type_registry,
        })? {
            vec.push(node)
        }

        Ok(vec)
    }
}
