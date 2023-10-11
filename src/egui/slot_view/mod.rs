mod slot_card;

use bevy::reflect::TypeRegistry;
use bevy_egui::egui::{self, Id, Ui};
use egui_phosphor::regular as icon;

use crate::{
    core::tree::{DynamicNode, ReflectDynamicNode},
    prelude::*,
};

const CARD_WIDTH: f32 = 190.0;
const CARD_MIN_HEIGHT: f32 = 64.0;
const ROW_PITCH: f32 = 130.0;
const H_GAP: f32 = 40.0;
const DOT_SPACING: f32 = 28.0;
const DOT_RADIUS: f32 = 1.3;
const SLOT_LABEL_GAP: f32 = 4.0;
const SLOT_LABEL_SIZE: f32 = 11.0;
const SLOT_LABEL_PADDING: egui::Vec2 = egui::vec2(5.0, 2.0);
const SLOT_LABEL_ROUNDING: f32 = 3.0;
const SLOT_LABEL_BG_COLOR: egui::Color32 = egui::Color32::from_gray(32);
const SLOT_LABEL_TEXT_COLOR: egui::Color32 = egui::Color32::from_gray(160);
const BACKGROUND_COLOR: egui::Color32 = egui::Color32::from_gray(18);
const DOT_COLOR: egui::Color32 = egui::Color32::from_gray(32);
const CONNECTOR_COLOR: egui::Color32 = egui::Color32::from_gray(90);

/// Slot-based GUI view for editing a [`BehaviourTree`] with egui.
///
/// Renders the tree as an org chart on a pan/zoom canvas: nodes are laid out
/// with a tidy-tree algorithm and placed at absolute positions, connected by
/// lines. Each node is a card with its reflected fields inline; empty `+` slots
/// add children through the node library, the context menu removes/wraps a
/// node, and siblings are reordered by dragging a card's header handle.
///
/// Use [`BehaviourTreeSlotView::new`] to create a view, then
/// [`show`][BehaviourTreeSlotView::show] to show it in egui.
pub struct BehaviourTreeSlotView<'b, 't> {
    id: Id,
    tree: &'b mut BehaviourTree,
    registry: &'t TypeRegistry,
    library: &'b BehaviourTreeNodeLibrary,
}

enum SlotAction {
    Add { parent: NodeId, node: DynamicNode },
    InsertSibling { parent: NodeId, index: usize, node: DynamicNode },
    Move { node: NodeId, new_parent: NodeId },
    MoveTo { node: NodeId, parent: NodeId, index: usize },
    Wrap { node: NodeId, parent: NodeId, index: usize, wrapper: DynamicNode },
    WrapRoot { wrapper: DynamicNode },
    Change { node: NodeId, replacement: DynamicNode },
    Remove { node: NodeId },
    RemoveRoot,
}

#[derive(Clone, Copy)]
struct Sibling {
    parent: NodeId,
    index: usize,
    parent_has_room: bool,
}

#[derive(Clone, Copy)]
enum Slot {
    Node { id: NodeId, sibling: Option<Sibling> },
    Plus { parent: NodeId },
}

struct LayoutEntry {
    slot: Slot,
    slot_name: Option<String>,
    width: f32,
    depth: usize,
    parent: Option<usize>,
    children: Vec<usize>,
    center_x: f32,
    rect: egui::Rect,
}

/// Left/right x-extent of a subtree per depth level, relative to its root's
/// center.
struct Contour {
    left: Vec<f32>,
    right: Vec<f32>,
}

impl<'b, 't> BehaviourTreeSlotView<'b, 't> {
    /// Creates a new slot-based behaviour tree view.
    pub fn new(
        tree: &'b mut BehaviourTree,
        library: &'b BehaviourTreeNodeLibrary,
        registry: &'t TypeRegistry,
        id: Id,
    ) -> Self {
        Self { id, tree, registry, library }
    }

    /// Displays the editor, returning `true` if the tree was modified.
    pub fn show(&mut self, ui: &mut Ui) -> bool {
        let mut scene_rect = ui
            .memory_mut(|memory| memory.data.get_temp::<egui::Rect>(self.id))
            .unwrap_or(egui::Rect::ZERO);

        let root = self.tree.get_root_id();
        let mut arena: Vec<LayoutEntry> = Vec::new();
        self.build_layout(&mut arena, root, None, 0, None);
        layout_x(&mut arena, 0);
        let origin_x = arena[0].center_x;

        let mut actions = Vec::new();
        let mut changed = false;
        egui::Scene::new().zoom_range(0.1..=2.0).show(ui, &mut scene_rect, |ui| {
            paint_dot_grid(ui);

            let dragged = arena.iter().position(|entry| {
                matches!(entry.slot, Slot::Node { id, .. }
                    if ui.ctx().is_being_dragged(Id::new(("bt-slot-drag", id))))
            });
            let mut dimmed = vec![false; arena.len()];
            if let Some(index) = dragged {
                mark_subtree(&arena, index, &mut dimmed);
            }

            for i in 0..arena.len() {
                let (center_x, width, depth, slot, parent) = {
                    let entry = &arena[i];
                    (entry.center_x, entry.width, entry.depth, entry.slot, entry.parent)
                };
                let height = match slot {
                    Slot::Plus { .. } => {
                        row_card_height(&arena, parent, |c| arena[c].rect.height())
                    },
                    Slot::Node { .. } => CARD_MIN_HEIGHT,
                };
                let pos = egui::pos2((center_x - origin_x) - width * 0.5, depth as f32 * ROW_PITCH);
                let rect = egui::Rect::from_min_size(pos, egui::vec2(width, height));
                arena[i].rect = match slot {
                    Slot::Node { id, sibling } => {
                        self.card(ui, rect, id, sibling, dimmed[i], &mut actions, &mut changed)
                    },
                    Slot::Plus { parent } => {
                        self.plus_slot(ui, rect, parent, dimmed[i], &mut actions)
                    },
                };
            }

            for i in 0..arena.len() {
                if arena[i].children.is_empty() {
                    continue;
                }
                let child_rects: Vec<egui::Rect> =
                    arena[i].children.iter().map(|&c| arena[c].rect).collect();
                let alpha = if dimmed[i] { 0.5 } else { 1.0 };
                let stroke = egui::Stroke::new(1.5, CONNECTOR_COLOR.gamma_multiply(alpha));
                draw_connectors(ui.painter(), stroke, arena[i].rect, &child_rects);
            }

            for i in 0..arena.len() {
                if let Some(name) = &arena[i].slot_name {
                    paint_slot_name(ui.painter(), arena[i].rect, name, dimmed[i]);
                }
            }

            if let Some(index) = dragged {
                let Slot::Node { id, .. } = arena[index].slot else { return };
                egui::DragAndDrop::set_payload(ui.ctx(), id);
                let area_id = Id::new(("bt-slot-ghost", id));
                let layer_id = egui::LayerId::new(egui::Order::Tooltip, area_id);
                let mut top = egui::Rect::NOTHING;
                egui::Area::new(area_id)
                    .order(egui::Order::Tooltip)
                    .interactable(false)
                    .constrain(false)
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(ui.ctx(), |ui| {
                        top = self.ghost_subtree(ui, &arena, index, &mut changed);
                    });
                if let Some(pointer) = ui.ctx().pointer_interact_pos() {
                    let delta = pointer - top.center();
                    ui.ctx().transform_layer_shapes(
                        layer_id,
                        egui::emath::TSTransform::from_translation(delta),
                    );
                }
            }
        });

        ui.memory_mut(|memory| memory.data.insert_temp(self.id, scene_rect));

        let dirty = changed || !actions.is_empty();
        for action in actions {
            self.apply(action);
        }
        dirty
    }

    fn build_layout(
        &self,
        arena: &mut Vec<LayoutEntry>,
        id: NodeId,
        sibling: Option<Sibling>,
        depth: usize,
        parent: Option<usize>,
    ) -> usize {
        let me = arena.len();
        let slot_name = sibling.and_then(|sibling| self.slot_name(sibling.parent, sibling.index));
        arena.push(LayoutEntry {
            slot: Slot::Node { id, sibling },
            slot_name,
            width: CARD_WIDTH,
            depth,
            parent,
            children: Vec::new(),
            center_x: 0.0,
            rect: egui::Rect::NOTHING,
        });

        let children: Vec<NodeId> = self.tree.iter_children_id(id).collect();
        let empty_slots = self.available_slots(id).saturating_sub(children.len());
        let has_room = empty_slots > 0;

        let mut kids = Vec::new();
        for (index, child) in children.iter().enumerate() {
            let child_sibling = Sibling { parent: id, index, parent_has_room: has_room };
            kids.push(self.build_layout(arena, *child, Some(child_sibling), depth + 1, Some(me)));
        }
        for offset in 0..empty_slots {
            let plus = arena.len();
            arena.push(LayoutEntry {
                slot: Slot::Plus { parent: id },
                slot_name: self.slot_name(id, children.len() + offset),
                width: CARD_WIDTH,
                depth: depth + 1,
                parent: Some(me),
                children: Vec::new(),
                center_x: 0.0,
                rect: egui::Rect::NOTHING,
            });
            kids.push(plus);
        }
        arena[me].children = kids;
        me
    }

    fn ghost_subtree(
        &mut self,
        ui: &mut Ui,
        arena: &[LayoutEntry],
        root: usize,
        changed: &mut bool,
    ) -> egui::Rect {
        let base_x = arena[root].center_x;
        let base_depth = arena[root].depth;
        let mut subtree = Vec::new();
        collect_subtree(arena, root, &mut subtree);

        // The Area clips content left of x=0, so shift the subtree fully right.
        let left = subtree
            .iter()
            .map(|&i| (arena[i].center_x - base_x) - arena[i].width * 0.5)
            .fold(f32::INFINITY, f32::min);

        let mut rects = vec![egui::Rect::NOTHING; arena.len()];
        for &i in &subtree {
            let entry = &arena[i];
            let height = match entry.slot {
                Slot::Plus { .. } => row_card_height(arena, entry.parent, |c| rects[c].height()),
                Slot::Node { .. } => CARD_MIN_HEIGHT,
            };
            let pos = egui::pos2(
                (entry.center_x - base_x) - entry.width * 0.5 - left,
                (entry.depth - base_depth) as f32 * ROW_PITCH,
            );
            let rect = egui::Rect::from_min_size(pos, egui::vec2(entry.width, height));
            let builder = egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min));
            let slot = entry.slot;
            rects[i] = ui
                .scope_builder(builder, |ui| match slot {
                    Slot::Node { id, sibling } => {
                        self.card_contents(ui, id, sibling.map(|sibling| sibling.index), changed).1
                    },
                    Slot::Plus { .. } => plus_button(ui, height).rect,
                })
                .inner;
        }

        let stroke = egui::Stroke::new(1.5, CONNECTOR_COLOR);
        for &i in &subtree {
            if arena[i].children.is_empty() {
                continue;
            }
            let child_rects: Vec<egui::Rect> =
                arena[i].children.iter().map(|&c| rects[c]).collect();
            draw_connectors(ui.painter(), stroke, rects[i], &child_rects);
        }

        for &i in &subtree {
            if let Some(name) = &arena[i].slot_name {
                paint_slot_name(ui.painter(), rects[i], name, false);
            }
        }
        rects[root]
    }

    fn available_slots(&self, id: NodeId) -> usize {
        self.tree
            .get_node(id)
            .and_then(|node| self.registry.get_type_data::<ReflectDynamicNode>(node.type_id()))
            .map(|reflect_node| reflect_node.available_slots(id, self.tree))
            .unwrap_or(0)
    }

    fn slot_name(&self, parent: NodeId, index: usize) -> Option<String> {
        self.tree
            .get_node(parent)
            .and_then(|node| self.registry.get_type_data::<ReflectDynamicNode>(node.type_id()))
            .and_then(|reflect_node| reflect_node.slot_name(parent, index, self.tree))
            .map(|name| name.into_owned())
    }

    fn apply(&mut self, action: SlotAction) {
        match action {
            SlotAction::Add { parent, node } => {
                let child = self.tree.new_node_dyn(node);
                self.tree.append(parent, child);
            },
            SlotAction::InsertSibling { parent, index, node } => {
                let child = self.tree.new_node_dyn(node);
                self.tree.append(parent, child);
                let mut children: Vec<NodeId> = self.tree.iter_children_id(parent).collect();
                let from = children.len() - 1;
                let moved = children.remove(from);
                children.insert((index + 1).min(children.len()), moved);
                self.reorder(parent, children);
            },
            SlotAction::Move { node, new_parent } => {
                if new_parent == node
                    || self
                        .tree
                        .iter_descendants_id(node)
                        .any(|descendant| descendant == new_parent)
                {
                    return;
                }
                self.tree.detach(node);
                self.tree.append(new_parent, node);
            },
            SlotAction::MoveTo { node, parent, index } => {
                if node == parent
                    || self.tree.iter_descendants_id(node).any(|descendant| descendant == parent)
                {
                    return;
                }
                self.tree.detach(node);
                let mut children: Vec<NodeId> = self.tree.iter_children_id(parent).collect();
                children.insert(index.min(children.len()), node);
                self.reorder(parent, children);
            },
            SlotAction::Wrap { node, parent, index, wrapper } => {
                let wrapper = self.tree.new_node_dyn(wrapper);
                self.tree.detach(node);
                self.tree.append(wrapper, node);
                self.tree.append(parent, wrapper);
                let mut children: Vec<NodeId> = self.tree.iter_children_id(parent).collect();
                let from = children.len() - 1;
                let moved = children.remove(from);
                children.insert(index.min(children.len()), moved);
                self.reorder(parent, children);
            },
            SlotAction::Change { node, replacement } => {
                if let Some(dynamic) = self.tree.get_node_mut(node) {
                    *dynamic = replacement;
                }
            },
            SlotAction::WrapRoot { wrapper } => {
                let old_root = self.tree.get_root_id();
                let wrapper = self.tree.new_node_dyn(wrapper);
                self.tree.append(wrapper, old_root);
                self.tree.set_root(wrapper);
            },
            SlotAction::RemoveRoot => {
                let root = self.tree.get_root_id();
                let children: Vec<NodeId> = self.tree.iter_children_id(root).collect();
                let [child] = children[..] else { return };
                self.tree.detach(child);
                self.tree.remove_and_detach_children(root);
                self.tree.set_root(child);
            },
            SlotAction::Remove { node } => {
                let mut ids: Vec<NodeId> = self.tree.iter_descendants_id(node).collect();
                ids.push(node);
                for id in ids {
                    self.tree.remove_and_detach_children(id);
                }
            },
        }
    }

    fn reorder(&mut self, parent: NodeId, children: Vec<NodeId>) {
        for child in &children {
            self.tree.detach(*child);
        }
        for child in &children {
            self.tree.append(parent, *child);
        }
    }
}

fn paint_slot_name(painter: &egui::Painter, rect: egui::Rect, name: &str, dimmed: bool) {
    let alpha = if dimmed { 0.5 } else { 1.0 };
    let galley = painter.layout_no_wrap(
        name.to_owned(),
        egui::FontId::proportional(SLOT_LABEL_SIZE),
        SLOT_LABEL_TEXT_COLOR.gamma_multiply(alpha),
    );
    let anchor = rect.center_top() - egui::vec2(0.0, SLOT_LABEL_GAP);
    let text_rect = egui::Align2::CENTER_BOTTOM.anchor_size(anchor, galley.size());
    let background = text_rect.expand2(SLOT_LABEL_PADDING);
    painter.rect_filled(background, SLOT_LABEL_ROUNDING, SLOT_LABEL_BG_COLOR.gamma_multiply(alpha));
    painter.galley(text_rect.min, galley, SLOT_LABEL_TEXT_COLOR);
}

fn plus_button(ui: &mut Ui, height: f32) -> egui::Response {
    let outline = egui::Stroke::new(1.0, ui.visuals().weak_text_color());
    let widgets = &mut ui.visuals_mut().widgets;
    widgets.hovered.bg_fill = widgets.inactive.bg_fill;
    widgets.hovered.bg_stroke = outline;
    widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
    widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
    widgets.inactive.bg_stroke = outline;
    widgets.active.bg_stroke = outline;
    ui.add_sized([CARD_WIDTH, height], egui::Button::new(icon::PLUS))
}

fn row_card_height(
    arena: &[LayoutEntry],
    parent: Option<usize>,
    height_of: impl Fn(usize) -> f32,
) -> f32 {
    let Some(parent) = parent else { return CARD_MIN_HEIGHT };
    arena[parent]
        .children
        .iter()
        .filter(|&&c| matches!(arena[c].slot, Slot::Node { .. }))
        .map(|&c| height_of(c))
        .fold(CARD_MIN_HEIGHT, f32::max)
}

fn layout_x(arena: &mut [LayoutEntry], i: usize) -> Contour {
    let width = arena[i].width;
    let children = arena[i].children.clone();
    if children.is_empty() {
        arena[i].center_x = 0.0;
        return Contour { left: vec![-width * 0.5], right: vec![width * 0.5] };
    }

    let mut merged: Option<Contour> = None;
    for &child in &children {
        let contour = layout_x(arena, child);
        merged = Some(match merged {
            None => contour,
            Some(acc) => {
                let mut shift = acc.right[0] + H_GAP - contour.left[0];
                for depth in 0..contour.left.len().min(acc.right.len()) {
                    shift = shift.max(acc.right[depth] + H_GAP - contour.left[depth]);
                }
                shift_subtree(arena, child, shift);
                let shifted = Contour {
                    left: contour.left.iter().map(|x| x + shift).collect(),
                    right: contour.right.iter().map(|x| x + shift).collect(),
                };
                merge_contours(&acc, &shifted)
            },
        });
    }

    let first = arena[children[0]].center_x;
    let last = arena[*children.last().unwrap()].center_x;
    let center = (first + last) * 0.5;
    arena[i].center_x = center;

    let inner = merged.unwrap();
    let mut left = vec![center - width * 0.5];
    let mut right = vec![center + width * 0.5];
    left.extend_from_slice(&inner.left);
    right.extend_from_slice(&inner.right);
    Contour { left, right }
}

fn shift_subtree(arena: &mut [LayoutEntry], i: usize, dx: f32) {
    arena[i].center_x += dx;
    for child in arena[i].children.clone() {
        shift_subtree(arena, child, dx);
    }
}

fn merge_contours(a: &Contour, b: &Contour) -> Contour {
    let depth = a.left.len().max(b.left.len());
    let mut left = Vec::with_capacity(depth);
    let mut right = Vec::with_capacity(depth);
    for d in 0..depth {
        match (a.left.get(d), b.left.get(d)) {
            (Some(&al), Some(&bl)) => left.push(al.min(bl)),
            (Some(&al), None) => left.push(al),
            (None, Some(&bl)) => left.push(bl),
            (None, None) => left.push(0.0),
        }
        match (a.right.get(d), b.right.get(d)) {
            (Some(&ar), Some(&br)) => right.push(ar.max(br)),
            (Some(&ar), None) => right.push(ar),
            (None, Some(&br)) => right.push(br),
            (None, None) => right.push(0.0),
        }
    }
    Contour { left, right }
}

fn mark_subtree(arena: &[LayoutEntry], i: usize, dimmed: &mut [bool]) {
    dimmed[i] = true;
    for &child in &arena[i].children {
        mark_subtree(arena, child, dimmed);
    }
}

fn collect_subtree(arena: &[LayoutEntry], i: usize, out: &mut Vec<usize>) {
    out.push(i);
    for &child in &arena[i].children {
        collect_subtree(arena, child, out);
    }
}

fn draw_connectors(
    painter: &egui::Painter,
    stroke: egui::Stroke,
    parent: egui::Rect,
    children: &[egui::Rect],
) {
    if children.is_empty() {
        return;
    }
    let parent_bottom = parent.center_bottom();
    let row_top = children.iter().map(|rect| rect.top()).fold(f32::INFINITY, f32::min);
    let bus_y = (parent_bottom.y + row_top) * 0.5;

    painter.line_segment([parent_bottom, egui::pos2(parent_bottom.x, bus_y)], stroke);
    let min_x = children.iter().map(|rect| rect.center().x).fold(parent_bottom.x, f32::min);
    let max_x = children.iter().map(|rect| rect.center().x).fold(parent_bottom.x, f32::max);
    painter.line_segment([egui::pos2(min_x, bus_y), egui::pos2(max_x, bus_y)], stroke);
    for rect in children {
        let x = rect.center().x;
        painter.line_segment([egui::pos2(x, bus_y), egui::pos2(x, rect.top())], stroke);
    }
}

fn paint_dot_grid(ui: &Ui) {
    let visible = ui.clip_rect();
    if !visible.is_finite() || visible.is_negative() {
        return;
    }

    let painter = ui.painter();
    painter.rect_filled(visible, 0.0, BACKGROUND_COLOR);

    let mut spacing = DOT_SPACING;
    while visible.width() / spacing * visible.height() / spacing > 3000.0 {
        spacing *= 2.0;
    }

    let mut x = (visible.min.x / spacing).floor() * spacing;
    while x <= visible.max.x {
        let mut y = (visible.min.y / spacing).floor() * spacing;
        while y <= visible.max.y {
            painter.circle_filled(egui::pos2(x, y), DOT_RADIUS, DOT_COLOR);
            y += spacing;
        }
        x += spacing;
    }
}
