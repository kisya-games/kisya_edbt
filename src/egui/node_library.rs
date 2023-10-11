use std::any::TypeId;

use bevy::reflect::TypeRegistry;
use bevy_egui::egui::{self, Ui};
use egui_phosphor::regular as icon;

use crate::core::{
    BehaviourTreeNodeLibrary,
    node::NodeId,
    tree::{BehaviourTree, DynamicNode, ReflectDynamicNode},
};

const COLUMN_WIDTH: f32 = 140.0;

#[derive(Clone, Copy, PartialEq)]
enum Category {
    Composite,
    Decorator,
    Leaf,
    Custom,
}

impl Category {
    const ALL: [Category; 4] =
        [Category::Composite, Category::Decorator, Category::Leaf, Category::Custom];

    fn from_path(path: &str) -> Self {
        if path.starts_with("kisya_edbt::composite") {
            Self::Composite
        } else if path.starts_with("kisya_edbt::decorator") {
            Self::Decorator
        } else if path.starts_with("kisya_edbt::leaf") {
            Self::Leaf
        } else {
            Self::Custom
        }
    }

    fn header(self) -> (&'static str, &'static str) {
        match self {
            Self::Composite => (icon::TREE_STRUCTURE, "Composite"),
            Self::Decorator => (icon::SELECTION_ALL, "Decorator"),
            Self::Leaf => (icon::LEAF, "Leaf"),
            Self::Custom => (icon::PUZZLE_PIECE, "Custom"),
        }
    }
}

/// Categorized picker over a [`BehaviourTreeNodeLibrary`], laying nodes out in
/// per-category columns.
pub struct BehaviourTreeNodeLibraryPicker<'a> {
    library: &'a BehaviourTreeNodeLibrary,
    registry: &'a TypeRegistry,
    min_slots: Option<(&'a BehaviourTree, NodeId, usize)>,
}

impl<'a> BehaviourTreeNodeLibraryPicker<'a> {
    /// Create a new node library picker.
    pub fn new(library: &'a BehaviourTreeNodeLibrary, registry: &'a TypeRegistry) -> Self {
        Self { library, registry, min_slots: None }
    }

    /// Only offer nodes that can host at least `min` children when placed at
    /// `reference` in `tree`.
    pub fn with_min_slots(
        mut self,
        tree: &'a BehaviourTree,
        reference: NodeId,
        min: usize,
    ) -> Self {
        self.min_slots = Some((tree, reference, min));
        self
    }

    /// Show the picker and return a node if one was clicked.
    pub fn show(&self, ui: &mut Ui) -> Option<DynamicNode> {
        let entries = self.entries();
        let mut picked = None;
        ui.horizontal_top(|ui| {
            for category in Category::ALL {
                let column: Vec<&(Category, String, TypeId)> =
                    entries.iter().filter(|(c, ..)| *c == category).collect();
                if column.is_empty() {
                    continue;
                }
                ui.vertical(|ui| {
                    ui.set_width(COLUMN_WIDTH);
                    let (icon, name) = category.header();
                    ui.label(egui::RichText::new(format!("{icon}  {name}")).strong());
                    ui.separator();
                    for (_, name, type_id) in column {
                        if ui.selectable_label(false, name).clicked() {
                            picked = Some(*type_id);
                        }
                    }
                });
            }
        });
        picked.and_then(|type_id| DynamicNode::from_type_id(type_id, self.registry).ok())
    }

    /// Show the picker, updating `current` with the selected node.
    pub fn show_with_selection(
        &self,
        ui: &mut Ui,
        current: &mut Option<DynamicNode>,
    ) -> egui::Response {
        let entries = self.entries();
        let mut response = ui.response();
        ui.horizontal_top(|ui| {
            for category in Category::ALL {
                let column: Vec<&(Category, String, TypeId)> =
                    entries.iter().filter(|(c, ..)| *c == category).collect();
                if column.is_empty() {
                    continue;
                }
                ui.vertical(|ui| {
                    ui.set_width(COLUMN_WIDTH);
                    let (icon, name) = category.header();
                    ui.label(egui::RichText::new(format!("{icon}  {name}")).strong());
                    ui.separator();
                    for (_, name, type_id) in column {
                        let Ok(node) = DynamicNode::from_type_id(*type_id, self.registry) else {
                            continue;
                        };
                        response = response.union(ui.selectable_value(current, Some(node), name));
                    }
                });
            }
        });
        response
    }

    fn entries(&self) -> Vec<(Category, String, TypeId)> {
        let mut entries: Vec<(Category, String, TypeId)> = self
            .library
            .iter()
            .filter(|type_id| match self.min_slots {
                Some((tree, reference, min)) if min > 0 => {
                    self.registry.get_type_data::<ReflectDynamicNode>(**type_id).is_some_and(
                        |reflect_node| reflect_node.available_slots(reference, tree) >= min,
                    )
                },
                _ => true,
            })
            .filter_map(|type_id| {
                let registration = self.registry.get(*type_id)?;
                let path = registration.type_info().type_path();
                let name = registration.type_info().type_path_table().short_path().to_string();
                Some((Category::from_path(path), name, *type_id))
            })
            .collect();
        entries.sort_by(|(_, a, _), (_, b, _)| a.cmp(b));
        entries
    }
}
