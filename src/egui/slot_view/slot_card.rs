use bevy::reflect::ReflectRef;
use bevy_egui::egui::{self, Id, Ui};
use bevy_inspector_egui::reflect_inspector::{Context, InspectorUi};
use egui_phosphor::regular as icon;

use super::{BehaviourTreeSlotView, CARD_MIN_HEIGHT, CARD_WIDTH, Sibling, SlotAction, plus_button};
use crate::{core::tree::DynamicNode, egui::BehaviourTreeNodeLibraryPicker, prelude::*};

impl<'b, 't> BehaviourTreeSlotView<'b, 't> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn card(
        &mut self,
        ui: &mut Ui,
        rect: egui::Rect,
        id: NodeId,
        sibling: Option<Sibling>,
        dimmed: bool,
        actions: &mut Vec<SlotAction>,
        changed: &mut bool,
    ) -> egui::Rect {
        let index = sibling.map(|sibling| sibling.index);
        let builder =
            egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min));
        ui.scope_builder(builder, |ui| {
            if dimmed {
                ui.set_opacity(0.5);
                return self.card_contents(ui, id, index, changed).1;
            }

            let drag_id = Id::new(("bt-slot-drag", id));
            let bg_id = Id::new(("bt-slot-bg", id));
            let background = ui
                .memory(|memory| memory.data.get_temp::<egui::Rect>(bg_id))
                .map(|rect| ui.interact(rect, bg_id, egui::Sense::click()));

            let (handle, bounds) = self.card_contents(ui, id, index, changed);
            ui.memory_mut(|memory| memory.data.insert_temp(bg_id, bounds));

            let drop = ui.interact(bounds, Id::new(("bt-slot-drop", id)), egui::Sense::hover());
            if sibling.is_some()
                && egui::DragAndDrop::has_payload_of_type::<NodeId>(ui.ctx())
                && drop.contains_pointer()
            {
                let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
                ui.painter().rect_stroke(bounds, 4.0, stroke, egui::StrokeKind::Inside);
            }
            if let (Some(dragged), Some(sibling)) = (drop.dnd_release_payload::<NodeId>(), sibling)
            {
                actions.push(SlotAction::MoveTo {
                    node: *dragged,
                    parent: sibling.parent,
                    index: sibling.index,
                });
            }
            if sibling.is_some() {
                ui.interact(handle, drag_id, egui::Sense::drag())
                    .on_hover_cursor(egui::CursorIcon::Grab);
            }
            if let Some(background) = background {
                self.card_menu(&background, id, sibling, actions);
            }
            bounds
        })
        .inner
    }

    pub(super) fn plus_slot(
        &mut self,
        ui: &mut Ui,
        rect: egui::Rect,
        parent: NodeId,
        dimmed: bool,
        actions: &mut Vec<SlotAction>,
    ) -> egui::Rect {
        let height = rect.height();
        let builder =
            egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::top_down(egui::Align::Min));
        ui.scope_builder(builder, |ui| {
            if dimmed {
                ui.set_opacity(0.5);
            }
            let response = plus_button(ui, height);
            if egui::DragAndDrop::has_payload_of_type::<NodeId>(ui.ctx())
                && response.contains_pointer()
            {
                let stroke = egui::Stroke::new(2.0, ui.visuals().selection.stroke.color);
                ui.painter().rect_stroke(response.rect, 2.0, stroke, egui::StrokeKind::Inside);
            }
            if let Some(dragged) = response.dnd_release_payload::<NodeId>() {
                actions.push(SlotAction::Move { node: *dragged, new_parent: parent });
            }
            if !dimmed {
                egui::Popup::menu(&response)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClick)
                    .show(|ui| {
                        if let Some(node) = self.node_picker(ui, parent, 0) {
                            actions.push(SlotAction::Add { parent, node });
                        }
                    });
            }
            response.rect
        })
        .inner
    }

    pub(super) fn card_contents(
        &mut self,
        ui: &mut Ui,
        id: NodeId,
        index: Option<usize>,
        changed: &mut bool,
    ) -> (egui::Rect, egui::Rect) {
        ui.push_id(id, |ui| self.card_frame(ui, id, index, changed)).inner
    }

    fn card_menu(
        &mut self,
        response: &egui::Response,
        id: NodeId,
        sibling: Option<Sibling>,
        actions: &mut Vec<SlotAction>,
    ) {
        response.context_menu(|ui| {
            let add_label = format!("{}  Add sibling", icon::PLUS);
            match sibling {
                Some(sibling) if sibling.parent_has_room => {
                    ui.menu_button(add_label, |ui| {
                        if let Some(node) = self.node_picker(ui, sibling.parent, 0) {
                            actions.push(SlotAction::InsertSibling {
                                parent: sibling.parent,
                                index: sibling.index,
                                node,
                            });
                            ui.close();
                        }
                    });
                },
                _ => {
                    ui.add_enabled(false, egui::Button::new(add_label));
                },
            }

            let wrap_label = format!("{}  Wrap in", icon::STACK);
            ui.menu_button(wrap_label, |ui| {
                if let Some(wrapper) = self.node_picker(ui, id, 1) {
                    actions.push(match sibling {
                        Some(sibling) => SlotAction::Wrap {
                            node: id,
                            parent: sibling.parent,
                            index: sibling.index,
                            wrapper,
                        },
                        None => SlotAction::WrapRoot { wrapper },
                    });
                    ui.close();
                }
            });

            let change_label = format!("{}  Change node", icon::SWAP);
            let min_children = self.tree.get_children_len(id);
            ui.menu_button(change_label, |ui| {
                if let Some(replacement) = self.node_picker(ui, id, min_children) {
                    actions.push(SlotAction::Change { node: id, replacement });
                    ui.close();
                }
            });

            let remove_label = format!("{}  Remove", icon::TRASH);
            match sibling {
                Some(_) => {
                    if ui.button(remove_label).clicked() {
                        actions.push(SlotAction::Remove { node: id });
                        ui.close();
                    }
                },
                None if self.tree.get_children_len(id) == 1 => {
                    if ui.button(remove_label).clicked() {
                        actions.push(SlotAction::RemoveRoot);
                        ui.close();
                    }
                },
                None => {
                    ui.add_enabled(false, egui::Button::new(remove_label)).on_disabled_hover_text(
                        "Root can only be removed when it has a single child",
                    );
                },
            }
        });
    }

    fn node_picker(
        &self,
        ui: &mut Ui,
        reference: NodeId,
        min_children: usize,
    ) -> Option<DynamicNode> {
        BehaviourTreeNodeLibraryPicker::new(self.library, self.registry)
            .with_min_slots(self.tree, reference, min_children)
            .show(ui)
    }

    fn card_frame(
        &mut self,
        ui: &mut Ui,
        id: NodeId,
        index: Option<usize>,
        changed: &mut bool,
    ) -> (egui::Rect, egui::Rect) {
        let mut handle = egui::Rect::NOTHING;
        let frame = egui::Frame::popup(ui.style()).shadow(egui::Shadow::NONE).show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
            ui.set_width(CARD_WIDTH);
            ui.set_min_height(CARD_MIN_HEIGHT);
            let name = self
                .tree
                .get_node(id)
                .map(|node| node.short_name().to_string())
                .unwrap_or_default();

            let row = ui.horizontal(|ui| {
                let grip = match index {
                    Some(_) => Some(
                        ui.add(
                            egui::Label::new(egui::RichText::new(icon::DOTS_SIX_VERTICAL).strong())
                                .selectable(false),
                        )
                        .rect,
                    ),
                    None => {
                        ui.add(
                            egui::Label::new(egui::RichText::new(icon::MAP_PIN_SIMPLE).strong())
                                .selectable(false),
                        );
                        None
                    },
                };
                ui.add(egui::Label::new(egui::RichText::new(name).strong()).selectable(false));
                let tag = match index {
                    Some(index) => format!("#{}", index + 1),
                    None => "Root".to_string(),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::Label::new(egui::RichText::new(tag).weak()).selectable(false));
                });
                grip
            });
            handle = row.inner.unwrap_or(egui::Rect::NOTHING);

            let has_fields =
                self.tree.get_node(id).is_some_and(|node| match node.get().reflect_ref() {
                    ReflectRef::Struct(value) => value.field_len() > 0,
                    ReflectRef::TupleStruct(value) => value.field_len() > 0,
                    ReflectRef::Tuple(value) => value.field_len() > 0,
                    ReflectRef::Enum(_) => true,
                    _ => false,
                });

            if has_fields {
                ui.separator();
                if let Some(dynamic) = self.tree.get_node_mut(id) {
                    let mut cx = Context { world: None, queue: None };
                    if InspectorUi::for_bevy(self.registry, &mut cx)
                        .ui_for_reflect(dynamic.get_mut(), ui)
                    {
                        *changed = true;
                    }
                }
            }
        });
        (handle, frame.response.rect)
    }
}
