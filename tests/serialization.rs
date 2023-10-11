use bevy::{asset::AssetPlugin, prelude::*};
use kisya_edbt::BehaviourPlugins;

#[test]
fn built_in_nodes_survive_serde() {
    // Node registration round-trips every node through the tree asset serializer in
    // debug builds, so building the app is the whole test.
    App::new().add_plugins((MinimalPlugins, AssetPlugin::default(), BehaviourPlugins));
}
