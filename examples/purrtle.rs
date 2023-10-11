//! Purrtle: simple [turtle graphics][turtle] clone driven by a `kisya_edbt`
//! behaviour tree.
//!
//! The window is split in half:
//! - The top is the behaviour-tree editor (`kisya_edbt`'s egui
//!   `BehaviourTreeSlotView`)
//! - The bottom is the turtle acting out the tree, drawn with retained gizmos.
//!
//! Custom leaf nodes `Forward`, `Rotate` and `Pen` are all it takes to make it
//! draw. When the tree finishes, the turtle is despawned and a fresh one spawns
//! back at the origin, so editing the tree reshapes the next run.
//!
//! [turtle]: https://en.wikipedia.org/wiki/Turtle_graphics

use bevy::{
    camera::Viewport,
    feathers::{
        FeathersPlugins,
        controls::{ButtonVariant, FeathersButton, FeathersSlider},
        dark_theme::create_dark_theme,
        theme::{ThemeBackgroundColor, ThemedText, UiTheme},
        tokens,
    },
    gizmos::config::GizmoLineConfig,
    prelude::*,
    ui_widgets::{Activate, SliderPrecision, SliderStep, ValueChange, slider_self_update},
    window::WindowResized,
};
use bevy_egui::{
    EguiContext, EguiContexts, EguiGlobalSettings, EguiPrimaryContextPass, PrimaryEguiContext, egui,
};
use kisya_edbt::{egui::BehaviourTreeSlotView, prelude::*};

const SPEED_MAX: f32 = 10.0;
const LINE_SIZE: f32 = 5.0;
const TURTLE_RADIUS: f32 = 12.0;
const STEP_SIZE: f32 = 15.0;
const STEP_INTERVAL: f32 = 0.05;
const TURN_SPEED: f32 = 8.0;
const TRAIL_COLOR: Color = Color::srgb(0.894, 0.0, 0.275);
const TURTLE_COLOR: Color = Color::srgb(0.451, 0.333, 0.922);

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window { title: "Purrtle".into(), ..default() }),
                ..default()
            }),
            FeathersPlugins,
            BehaviourPlugins,
            PurrtlePlugin,
        ))
        .insert_resource(UiTheme(create_dark_theme()))
        .run();
}

struct PurrtlePlugin;

impl Plugin for PurrtlePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<ForwardNode>().with_system(forward_node_system).register();
        app.add_behaviour_node::<RotateNode>().with_system(rotate_node_system).register();
        app.add_behaviour_node::<PenNode>().with_setup_observer(on_pen_setup_hook).register();

        app.add_systems(Startup, setup_system)
            .add_systems(Update, update_viewports_system)
            .add_systems(EguiPrimaryContextPass, editor_system)
            .add_observer(on_turtle_done_hook)
            .add_observer(on_egui_context_added_hook);
    }
}

#[derive(Resource)]
struct PurrtleApp {
    tree: Handle<BehaviourTree>,
}

#[derive(SceneComponent, Clone, Default)]
struct Turtle {
    heading: f32,
    pen: bool,
}

impl Turtle {
    pub fn scene() -> impl Scene {
        bsn! {
            #Turtle
            template(|ctx| {
                let mut gizmos = ctx.resource_mut::<Assets<GizmoAsset>>();
                let mut gizmo = GizmoAsset::default();
                gizmo.circle_2d(Vec2::ZERO, TURTLE_RADIUS, TURTLE_COLOR);
                gizmo.line_2d(Vec2::ZERO, Vec2::X * TURTLE_RADIUS * 1.6, TURTLE_COLOR);
                Ok(Gizmo { handle : gizmos.add(gizmo), line_config: GizmoLineConfig { width: LINE_SIZE,..default() }, depth_bias: 1.0 })
            })
            template(|ctx| {
                Ok(Behaviour { tree: ctx.resource::<PurrtleApp>().tree.clone() })
            })
        }
    }
}

#[derive(SceneComponent, Clone, Default)]
struct Canvas;

impl Canvas {
    pub fn scene() -> impl Scene {
        bsn! {
            #Canvas
            template(|ctx| {
                let mut gizmos = ctx.resource_mut::<Assets<GizmoAsset>>();
                let gizmo = GizmoAsset::default();
                Ok(Gizmo { handle : gizmos.add(gizmo), line_config: GizmoLineConfig { width: LINE_SIZE,..default() }, depth_bias: 0.0 })
            })
        }
    }
}

/// Leaf node: move the turtle forward `steps` discrete steps along its heading.
#[derive(Reflect, Default, Clone, Copy)]
struct ForwardNode {
    steps: u8,
}

impl ForwardNode {
    fn new(steps: u8) -> Self { Self { steps } }
}

/// Task for [`ForwardNode`].
#[derive(Reflect, Default)]
struct ForwardTask {
    remaining: u8,
    timer: Timer,
}

impl BehaviourNode for ForwardNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ForwardTask;

    fn build_task(&self) -> Self::Task {
        ForwardTask {
            remaining: self.steps,
            timer: Timer::from_seconds(STEP_INTERVAL, TimerMode::Repeating),
        }
    }
}

/// Leaf node: turn the turtle in place by `degrees` (positive is
/// counterclockwise).
#[derive(Reflect, Default, Clone, Copy)]
struct RotateNode {
    degrees: f32,
}

impl RotateNode {
    fn new(degrees: f32) -> Self { Self { degrees } }
}

/// Task for [`RotateNode`].
#[derive(Reflect, Default)]
struct RotateTask {
    remaining: f32,
}

impl BehaviourNode for RotateNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = RotateTask;

    fn build_task(&self) -> Self::Task { RotateTask { remaining: self.degrees.to_radians() } }
}

/// Leaf node: raise or lower the pen, deciding whether motion leaves a trail.
#[derive(Reflect, Default, Clone, Copy)]
struct PenNode {
    down: bool,
}

impl PenNode {
    fn down() -> Self { Self { down: true } }

    fn up() -> Self { Self { down: false } }
}

impl BehaviourNode for PenNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ();

    fn build_task(&self) -> Self::Task {}
}

fn setup_system(
    mut commands: Commands,
    mut trees: ResMut<Assets<BehaviourTree>>,
    mut egui_settings: ResMut<EguiGlobalSettings>,
) {
    egui_settings.auto_create_primary_context = false;

    let tree = trees.add(behaviour_tree! {
        SequenceNode => [
            PenNode::down(),
            LoopForNode::times(8) => [ SequenceNode => [
                LoopForNode::times(8) => [
                    SequenceNode => [ ForwardNode::new(2), RotateNode::new(45.0) ]
                ],
                RotateNode::new(45.0)
            ]],
            PenNode::up(),
            WaitNode::time(1.0)
        ]
    });
    commands.insert_resource(PurrtleApp { tree });

    let turtle_camera = commands.spawn(Camera2d).id();
    commands.spawn((Camera2d, Camera { order: 1, ..default() }, PrimaryEguiContext));

    commands.spawn_scene_list(bsn_list![@Canvas, @Turtle]);
    commands.spawn_scene(controls_ui()).insert(UiTargetCamera(turtle_camera));
}

fn update_viewports_system(
    window: Single<&Window>,
    mut resized: MessageReader<WindowResized>,
    mut turtle_camera: Single<&mut Camera, Without<PrimaryEguiContext>>,
    mut egui_camera: Single<&mut Camera, With<PrimaryEguiContext>>,
) {
    if resized.is_empty() {
        return;
    }
    resized.clear();

    let size = window.physical_size();
    let top = size.y / 2;

    egui_camera.viewport = Some(Viewport {
        physical_position: UVec2::ZERO,
        physical_size: UVec2::new(size.x, top),
        ..default()
    });
    turtle_camera.viewport = Some(Viewport {
        physical_position: UVec2::new(0, top),
        physical_size: UVec2::new(size.x, size.y - top),
        ..default()
    });
}

fn forward_node_system(
    mut cmd: Commands,
    time: Res<Time>,
    mut assets: ResMut<Assets<GizmoAsset>>,
    canvas: Single<&Gizmo, With<Canvas>>,
    mut q_task: Query<TaskMut<ForwardNode>>,
    mut q_turtle: Query<(&mut Transform, &Turtle)>,
) {
    for mut task in &mut q_task {
        let (entity, actor) = (task.entity(), task.actor());
        let Ok((mut transform, turtle)) = q_turtle.get_mut(actor) else {
            continue;
        };

        task.timer.tick(time.delta());
        for _ in 0..task.timer.times_finished_this_tick() {
            if task.remaining == 0 {
                break;
            }
            let from = transform.translation.truncate();
            transform.translation += (Vec2::from_angle(turtle.heading) * STEP_SIZE).extend(0.0);
            task.remaining -= 1;

            if turtle.pen
                && let Some(mut trail) = assets.get_mut(&canvas.handle)
            {
                trail.line_2d(from, transform.translation.truncate(), TRAIL_COLOR);
            }
        }

        if task.remaining == 0 {
            cmd.entity(entity).insert(TaskStatus::Success);
        }
    }
}

fn rotate_node_system(
    mut cmd: Commands,
    time: Res<Time>,
    mut q_task: Query<TaskMut<RotateNode>>,
    mut q_turtle: Query<(&mut Transform, &mut Turtle)>,
) {
    for mut task in &mut q_task {
        let (entity, actor) = (task.entity(), task.actor());
        let Ok((mut transform, mut turtle)) = q_turtle.get_mut(actor) else {
            continue;
        };

        let step = (TURN_SPEED * time.delta_secs()).min(task.remaining.abs());
        turtle.heading += task.remaining.signum() * step;
        task.remaining -= task.remaining.signum() * step;
        transform.rotation = Quat::from_rotation_z(turtle.heading);

        if task.remaining.abs() <= f32::EPSILON {
            cmd.entity(entity).insert(TaskStatus::Success);
        }
    }
}

fn on_pen_setup_hook(
    event: On<Add, TaskWorker<PenNode>>,
    mut cmd: Commands,
    q_task: Query<(TaskRef<PenNode>, NodeRef<PenNode>)>,
    mut q_turtle: Query<&mut Turtle>,
) -> Result<()> {
    let (task, node) = q_task.get(event.entity)?;
    let mut turtle = q_turtle.get_mut(task.actor())?;
    turtle.pen = node.down;
    cmd.entity(event.entity).insert(TaskStatus::Success);
    Ok(())
}

fn on_turtle_done_hook(
    event: On<Remove, Behaviour>,
    mut cmd: Commands,
    mut gizmos: ResMut<Assets<GizmoAsset>>,
    canvas: Single<&Gizmo, With<Canvas>>,
) {
    cmd.entity(event.entity).despawn();
    if let Some(mut trail) = gizmos.get_mut(&canvas.handle) {
        trail.clear();
    }
    cmd.spawn_scene(bsn! { @Turtle });
}

fn controls_ui() -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            bottom: px(16),
            left: px(0),
            right: px(0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::Center,
        }
        Children [(
            Node {
                flex_direction: FlexDirection::Row,
                align_items: AlignItems::Center,
                column_gap: px(12),
                padding: {UiRect::axes(px(16), px(10))},
                border_radius: {BorderRadius::all(px(8))},
            }
            ThemeBackgroundColor(tokens::WINDOW_BG)
            Children [
                (Text("Speed") ThemedText),
                (
                    Node { width: px(220) }
                    @FeathersSlider { @value: 1.0, @min: 0.0, @max: {SPEED_MAX} }
                    SliderStep(0.1)
                    SliderPrecision(1)
                    on(slider_self_update)
                    on(|change: On<ValueChange<f32>>, mut time: ResMut<Time<Virtual>>| {
                        time.set_relative_speed(change.value.max(0.0));
                    })
                ),
                (
                    @FeathersButton {
                        @caption: bsn! { Text("Respawn") ThemedText },
                        @variant: {ButtonVariant::Primary}
                    }
                    on(|_activate: On<Activate>,
                        mut commands: Commands,
                        q_turtle: Query<Entity, With<Turtle>>| {
                        for entity in &q_turtle {
                            commands.entity(entity).remove::<Behaviour>();
                        }
                    })
                )
            ]
        )]
    }
}

fn on_egui_context_added_hook(
    event: On<Add, EguiContext>,
    mut q_ctx: Query<&mut EguiContext>,
) -> Result<()> {
    let mut ctx = q_ctx.get_mut(event.entity)?;
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.get_mut().set_fonts(fonts);
    Ok(())
}

fn editor_system(
    mut contexts: EguiContexts,
    mut trees: ResMut<Assets<BehaviourTree>>,
    app: Res<PurrtleApp>,
    library: Res<BehaviourTreeNodeLibrary>,
    registry: Res<AppTypeRegistry>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let Some(mut tree) = trees.get_mut(&app.tree) else {
        return;
    };

    let registry = registry.read();
    let mut root = egui::Ui::new(
        ctx.clone(),
        "purrtle-root".into(),
        egui::UiBuilder::new().layer_id(egui::LayerId::background()).max_rect(ctx.viewport_rect()),
    );
    BehaviourTreeSlotView::new(&mut tree, &library, &registry, egui::Id::new("purrtle-tree"))
        .show(&mut root);
}
