mod common;

use common::*;
use kisya_edbt::prelude::*;

#[test]
fn empty_runner() {
    let mut harness = TestHarness::new();
    harness.update();

    assert_eq!(harness.cycles(), 0, "empty runner should not have any ran cycles");
}

#[test]
fn tasks_despawn_with_their_actor() {
    let mut harness = TestHarness::new();
    let actor = harness.spawn(behaviour_tree! {
        SequenceNode => [
            ProbeNode::once("a", TaskStatus::Success, 1000),
            ProbeNode::once("b", TaskStatus::Success, 1000)
        ]
    });
    harness.update();

    let running_tasks = harness.tasks();
    assert!(
        running_tasks.len() == 2,
        "only SequenceNode and its first child should be running, got {running_tasks:?} running tasks"
    );

    harness.world_mut().despawn(actor);

    for task in running_tasks {
        assert!(
            harness.world().get_entity(task).is_err(),
            "task {task:?} should have been despawned along with its actor"
        );
    }
}

#[test]
fn disabled_behaviour_component_with_initial_spawn() {
    let mut harness = TestHarness::new();
    let tree = harness.add_tree(behaviour_tree! {
        SequenceNode => [
            ProbeNode::once("a", TaskStatus::Success, 1000),
            ProbeNode::once("b", TaskStatus::Success, 1000)
        ]
    });
    let actor =
        harness.world_mut().spawn((Behaviour { tree: tree.clone() }, DisabledBehaviour)).id();
    harness.update();

    let (total, disabled) = harness.task_count();
    assert!(total >= 1, "the disabled tree should still have spawned its tasks");
    assert_eq!(
        total, disabled,
        "every task of a just spawned tree with DisableBehaviour must start disabled"
    );
    assert_eq!(
        harness.cycles(),
        0,
        "just spawned tree with DisabledBehaviour doesn't trigger runner cycles"
    );
    assert!(
        harness.world().get::<Behaviour>(actor).is_some(),
        "just spawned tree with DisabledBehaviour should still be alive and have Behaviour"
    );
}

#[test]
fn disabled_behaviour_component_with_late_insert() {
    let mut harness = TestHarness::new();
    let actor = harness.spawn(behaviour_tree! {
        SequenceNode => [
            ProbeNode::once("a", TaskStatus::Success, 1000),
            ProbeNode::once("b", TaskStatus::Success, 1000)
        ]
    });
    harness.update();

    let (total, disabled) = harness.task_count();
    assert!(total >= 1, "expected running tasks, got {total}");
    assert_eq!(disabled, 0, "nothing is disabled before DisabledBehaviour");

    harness.world_mut().entity_mut(actor).insert(DisabledBehaviour);
    let (total, disabled) = harness.task_count();
    assert_eq!(total, disabled, "every task is disabled while DisabledBehaviour is present");

    harness.world_mut().entity_mut(actor).remove::<DisabledBehaviour>();
    let (_, disabled) = harness.task_count();
    assert_eq!(disabled, 0, "removing DisabledBehaviour re-enables every task");
}
