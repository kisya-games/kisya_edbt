mod common;

use common::*;
use kisya_edbt::prelude::*;

#[test]
fn sequence_node() {
    assert_eq!(
        complete(behaviour_tree! {
            SequenceNode => [
                ProbeNode::once("a", TaskStatus::Success, 0),
                ProbeNode::once("b", TaskStatus::Success, 0),
                ProbeNode::once("c", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![
            +a, a => success,
            +b, b => success,
            +c, c => success,
            SequenceNode => success,
        ],
        "all children succeed => they run in order, and so the sequence also succeeds"
    );

    assert_eq!(
        complete(behaviour_tree! {
            SequenceNode => [
                ProbeNode::once("a", TaskStatus::Success, 0),
                ProbeNode::once("b", TaskStatus::Failure, 0),
                ProbeNode::once("c", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![
            +a, a => success,
            +b, b => failure,
            SequenceNode => failure,
        ],
        "2nd child fails => the sequence stops immediately and fails also, later siblings never run"
    );

    assert_eq!(
        complete(behaviour_tree! { SequenceNode }),
        lifecycle![SequenceNode => success],
        "no children => the sequence succeeds instantly"
    );
}

#[test]
fn condition_node() {
    assert_eq!(
        complete(behaviour_tree! {
            ConditionNode => [
                ProbeNode::once("cond", TaskStatus::Success, 0),
                ProbeNode::once("true_branch", TaskStatus::Failure, 0),
                ProbeNode::once("else_branch", TaskStatus::Failure, 0)
            ]
        }),
        lifecycle![
            +cond, cond => success,
            +true_branch, true_branch => failure,
            ConditionNode => failure,
        ],
        "condition succeeds => only the true branch runs, and its status is propagated"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ConditionNode => [
                ProbeNode::once("cond", TaskStatus::Failure, 0),
                ProbeNode::once("true_branch", TaskStatus::Success, 0),
                ProbeNode::once("else_branch", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![
            +cond, cond => failure,
            +else_branch, else_branch => success,
            ConditionNode => success,
        ],
        "condition fails, else branch exists => only the else branch runs, and its status is propagated"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ConditionNode => [
                ProbeNode::once("cond", TaskStatus::Failure, 0),
                ProbeNode::once("true_branch", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![+cond, cond => failure, ConditionNode => failure],
        "condition fails, no else branch => neither branch runs, and the node fails outright"
    );
}

#[test]
#[cfg(feature = "random")]
fn one_of_node() {
    assert_eq!(
        complete(behaviour_tree! {
            OneOfNode::weighted(vec![1.0, 0.0]) => [
                ProbeNode::once("chosen", TaskStatus::Success, 0),
                ProbeNode::once("never", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![+chosen, chosen => success, OneOfNode => success],
        "a zero-weight sibling is never picked, and the chosen child's success propagates"
    );

    // TODO: how to properly test random ? I don't know...
}

#[test]
fn parallel_node() {
    assert_eq!(
        complete_in(
            behaviour_tree! {
                ParallelNode::all_succeed() => [
                    ProbeNode::once("a", TaskStatus::Success, 2),
                    ProbeNode::once("b", TaskStatus::Success, 2)
                ]
            },
            2,
        ),
        lifecycle![
            +a, +b, ~a, ~b, ~a, ~b,
            a => success, b => success,
            ParallelNode => success,
        ],
        "both children finish ticking => the parent succeeds"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ParallelNode::any_succeed() => [
                ProbeNode::once("fast", TaskStatus::Success, 1),
                ProbeNode::once("slow", TaskStatus::Success, 10)
            ]
        }),
        lifecycle![
            +fast, +slow, ~fast, ~slow,
            fast => success,
            ParallelNode => success,
            slow => success,
        ],
        "faster child succeeds => the parent finishes immediately and force-finishes other task before it ticks"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ParallelNode::all_succeed().strict() => [
                ProbeNode::once("a", TaskStatus::Success, 0),
                ProbeNode::once("b", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![
            +a, a => success,
            +b, b => success,
            ParallelNode => success,
        ],
        "strict evaluation, all_succeed => spawns and waits for all children to succeed"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ParallelNode::any_failed().strict() => [
                ProbeNode::once("a", TaskStatus::Failure, 0),
                ProbeNode::once("b", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![
            +a, a => failure,
            +b, b => success,
            ParallelNode => success,
        ],
        "strict evaluation, any_failed => spawns all children, even though the first one failed"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ParallelNode::all_failed().lazy() => [
                ProbeNode::once("a", TaskStatus::Failure, 0),
                ProbeNode::once("b", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![
            +a, a => failure,
            +b, b => success,
            ParallelNode => failure,
        ],
        "lazy evaluation, all_failed => spawns children one by one and bails on the second"
    );

    assert_eq!(
        complete(behaviour_tree! {
            ParallelNode::any_succeed().lazy() => [
                ProbeNode::once("a", TaskStatus::Success, 0),
                ProbeNode::once("b", TaskStatus::Success, 0)
            ]
        }),
        lifecycle![+a, a => success, ParallelNode => success],
        "lazy evaluation, any_succeed => spawns children one by one and bails on the first succeeded"
    );
}

#[test]
fn while_node() {
    assert_eq!(
        complete_in(
            behaviour_tree! {
                WhileNode::while_success() => [
                    ProbeNode::serial("cond", vec![TaskStatus::Success, TaskStatus::Success, TaskStatus::Failure], 0),
                    ProbeNode::once("action", TaskStatus::Success, 0)
                ]
            },
            3,
        ),
        lifecycle![
            +cond, cond => success, +action, action => success,
            +cond, cond => success, +action, action => success,
            +cond, cond => failure,
            WhileNode => success,
        ],
        "action reruns for as long as the condition keeps succeeding"
    );
}
