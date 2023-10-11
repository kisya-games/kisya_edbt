//! Set of systems and schedulers to run a [`BehaviourTree`].

use bevy::{
    ecs::schedule::{ScheduleBuildSettings, ScheduleLabel},
    prelude::*,
};

use crate::core::{
    Behaviour, DisabledBehaviour,
    node::TreeNodeId,
    spawn::SpawnTask,
    task::{DisabledTask, TaskPool},
    tree::BehaviourTree,
};

/// Plugin for adding new [`Behaviours`][Behaviour] to the runner and then
/// running behaviour schedulers.
pub struct BehaviourRunnerPlugin;

impl Plugin for BehaviourRunnerPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BehaviourRunnerCycles>();

        let mut update_schedule = Schedule::new(BehaviourUpdateCycleSchedule);
        update_schedule
            .set_build_settings(ScheduleBuildSettings {
                // Cycle update may spawn new tasks, and PostUpdate will put them to sleep
                // instantly, if we allow bevy to insert apply_deferred after Update.
                // apply_deferred should run at schedule end.
                auto_insert_apply_deferred: false,
                ..default()
            })
            .configure_sets(
                (BehaviourUpdateCycleSystems::Update, BehaviourUpdateCycleSystems::PostUpdate)
                    .chain(),
            );
        let new_run_schedule = Schedule::new(BehaviourNewRunSchedule);
        let clear_schedule = Schedule::new(BehaviourEndRunSchedule);
        app.add_schedule(clear_schedule)
            .add_schedule(new_run_schedule)
            .add_schedule(update_schedule);

        // TODO: allow changing in which bevy schedule the whole thing runs
        app.add_systems(Update, run_behaviour_tasks_system)
            .add_systems(BehaviourNewRunSchedule, (spawn_new_behaviour_system, setup_cycles_system))
            .add_systems(BehaviourEndRunSchedule, despawn_finished_behaviour_system);
    }
}

/// Schedule for each new run (once per multiple update cycles).
#[derive(Debug, Clone, Copy, ScheduleLabel, PartialEq, Eq, Hash)]
pub(crate) struct BehaviourNewRunSchedule;

/// Schedule for one update cycle.
#[derive(Debug, Clone, Copy, ScheduleLabel, PartialEq, Eq, Hash)]
pub(crate) struct BehaviourUpdateCycleSchedule;

/// Schedule for finishing each run.
#[derive(Debug, Clone, Copy, ScheduleLabel, PartialEq, Eq, Hash)]
pub(crate) struct BehaviourEndRunSchedule;

/// System set for ordering systems inside [`BehaviourUpdateCycleSchedule`].
#[derive(SystemSet, Debug, Hash, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum BehaviourUpdateCycleSystems {
    Update,
    PostUpdate,
}

/// Information about behaviour update cycles.
#[derive(Resource, Default)]
pub struct BehaviourRunnerCycles {
    /// Amount of cycles run from the previous update of behaviour systems.
    pub(crate) count: usize,
    /// Run another cycle. This is good for oneshot/run-and-done nodes and
    /// composite nodes, so it wouldn't take several frames to continue
    /// updating the tree.
    pub(crate) rerun: bool,
}

impl BehaviourRunnerCycles {
    /// Return amount of cycles the previous update ran. Can be zero if there
    /// were no tasks at all.
    pub fn previous(&self) -> usize { self.count }
}

/// System to perform one behaviour run.
fn run_behaviour_tasks_system(world: &mut World) {
    world.run_schedule(BehaviourNewRunSchedule);

    loop {
        let mut cycles = world.resource_mut::<BehaviourRunnerCycles>();
        if cycles.rerun {
            cycles.rerun = false;
            cycles.count += 1;
        } else {
            break;
        }
        drop(cycles);

        world.run_schedule(BehaviourUpdateCycleSchedule);
    }

    world.run_schedule(BehaviourEndRunSchedule);
}

/// System to setup a new cycles run.
fn setup_cycles_system(
    q_actors: Query<(), (With<Behaviour>, Without<DisabledBehaviour>)>,
    mut cycles: ResMut<BehaviourRunnerCycles>,
) {
    // Having an enabled behaviour means two things:
    // 1. It is either a new behaviour and thus will spawn tasks in
    //    `spawn_new_behaviour_system`;
    // 2. Or a long-running behaviour that already has running tasks, otherwise it
    //    would be despawned in `despawn_finished_behaviour_system` .
    // Disabled behaviours are excluded: their tasks are skipped every cycle.
    let has_tasks = !q_actors.is_empty();

    cycles.rerun = has_tasks;
    cycles.count = 0;
}

/// System to add new or changed [`Behaviours`][Behaviour] to the behaviour
/// runner.
fn spawn_new_behaviour_system(
    mut commands: Commands,
    trees: Res<Assets<BehaviourTree>>,
    q_actor: Query<(Entity, &Behaviour, Has<DisabledBehaviour>), Changed<Behaviour>>,
) {
    for (entity, behaviour, is_disabled) in &q_actor {
        let Some(tree) = trees.get(&behaviour.tree) else {
            warn!("Invalid tree handle used in Behaviour component");
            continue;
        };

        // Re-inserting `Behaviour` should clear the old run before starting the new
        // one.
        commands.entity(entity).despawn_related::<TaskPool>();

        let tree_node_id = TreeNodeId { node: tree.get_root_id(), tree: behaviour.tree.id() };
        let root_entity = commands.spawn_empty().id();

        let mut commands = commands.entity(entity);
        commands.queue(SpawnTask {
            payload: smallvec::smallvec![(root_entity, tree_node_id)],
            parent: None,
        });
        if is_disabled {
            commands.queue(|entity: EntityWorldMut| {
                let pool = entity.get::<TaskPool>().cloned();
                let world = entity.into_world_mut();
                for task in pool.iter().flat_map(|p| p.iter()) {
                    world.entity_mut(task).insert(DisabledTask);
                }
            });
        }
    }
}

/// System to remove [`Behaviours`][Behaviour] whose task pool ran empty.
fn despawn_finished_behaviour_system(
    mut commands: Commands,
    q_actor: Query<Entity, (With<Behaviour>, Without<TaskPool>)>,
) {
    for entity in &q_actor {
        commands.entity(entity).remove::<Behaviour>();
    }
}
