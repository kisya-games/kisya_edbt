//! Egui-based editor for [`BehaviourTree`] assets.

mod node_library;
mod slot_view;

use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use bevy_inspector_egui::DefaultInspectorConfigPlugin;
pub use node_library::BehaviourTreeNodeLibraryPicker;
pub use slot_view::BehaviourTreeSlotView;

/// Plugin for BT egui editor.
pub struct BehaviourTreeEguiEditorPlugin;

impl Plugin for BehaviourTreeEguiEditorPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<EguiPlugin>() {
            app.add_plugins(EguiPlugin::default());
        }
        if !app.is_plugin_added::<DefaultInspectorConfigPlugin>() {
            app.add_plugins(DefaultInspectorConfigPlugin);
        }
    }
}
