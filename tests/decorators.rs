mod common;

use common::*;
use kisya_edbt::prelude::*;

#[test]
fn not_node() {
    assert_eq!(
        complete(behaviour_tree! { NotNode => [ProbeNode::once("child", TaskStatus::Success, 0)] }),
        lifecycle![+child, child => success, NotNode => failure],
        "flips a successful child into a failure"
    );

    assert_eq!(
        complete(behaviour_tree! { NotNode => [ProbeNode::once("child", TaskStatus::Failure, 0)] }),
        lifecycle![+child, child => failure, NotNode => success],
        "flips a failed child into a success"
    );
}

#[test]
fn loop_node() {
    assert_eq!(
        TestHarness::new().run_for(
            behaviour_tree! { LoopNode => [ProbeNode::once("child", TaskStatus::Success, 0)] },
            5
        ),
        (false, lifecycle![
            +child, child => success,
            +child, child => success,
            +child, child => success,
            +child, child => success,
            +child, child => success,
        ]),
        "the child reruns exactly once per update; LoopNode never finishes"
    );
}

#[test]
fn loop_until_node() {
    assert_eq!(
        complete_in(
            behaviour_tree! {
                LoopUntilNode::until_success() => [
                    ProbeNode::serial("child", vec![TaskStatus::Failure, TaskStatus::Failure, TaskStatus::Success], 0)
                ]
            },
            3,
        ),
        lifecycle![
            +child, child => failure,
            +child, child => failure,
            +child, child => success,
            LoopUntilNode => success,
        ],
        "reruns the child once per update until it succeeds, then finishes"
    );
}

#[test]
fn loop_for_node() {
    assert_eq!(
        complete_in(
            behaviour_tree! {
                LoopForNode::times(3) => [ProbeNode::once("child", TaskStatus::Success, 0)]
            },
            4,
        ),
        lifecycle![
            +child, child => success,
            +child, child => success,
            +child, child => success,
            LoopForNode => success,
        ],
        "reruns the child once per update for the given number of iterations, then finishes"
    );
}

#[test]
fn loop_for_node_stops_on_failure() {
    assert_eq!(
        complete_in(
            behaviour_tree! {
                LoopForNode::times(3) => [
                    ProbeNode::serial("child", vec![TaskStatus::Success, TaskStatus::Failure], 0)
                ]
            },
            2,
        ),
        lifecycle![
            +child, child => success,
            +child, child => failure,
            LoopForNode => failure,
        ],
        "a failing child stops the loop early and the node fails"
    );
}
