mod common;

use common::*;
use kisya_edbt::prelude::*;

fn started_count(log: &[LifecycleEvent]) -> usize {
    log.iter().filter(|e| matches!(e, LifecycleEvent::Started(_))).count()
}

fn finished_count(log: &[LifecycleEvent]) -> usize {
    log.iter().filter(|e| matches!(e, LifecycleEvent::Finished(..))).count()
}

fn ran(log: &[LifecycleEvent], label: &str) -> bool {
    log.iter().any(|e| matches!(e, LifecycleEvent::Started(l) if l.as_ref() == label))
}

#[test]
fn suspiciously_normal_looking_tree() {
    let (log, cycles) = complete_with_cycles(behaviour_tree! {
        SequenceNode => [
            ParallelNode::all_succeed() => [
                LoopUntilNode::until_success() => [
                    SequenceNode => [
                        ConditionNode => [
                            ProbeNode::once("cond", TaskStatus::Success, 0),
                            NotNode => [ProbeNode::once("then_leaf", TaskStatus::Failure, 0)],
                            ProbeNode::once("else", TaskStatus::Success, 0)
                        ],
                        ProbeNode::once("sib", TaskStatus::Success, 0)
                    ]
                ],
                ProbeNode::once("par_sib", TaskStatus::Success, 0)
            ],
            SequenceNode => [
                ProbeNode::once("q1", TaskStatus::Success, 0),
                ProbeNode::once("q2", TaskStatus::Success, 0)
            ]
        ]
    });

    assert!(ran(&log, "then_leaf"), "the deepest node is reached");
    assert_eq!(
        started_count(&log),
        6,
        "six leaves run: cond, then_leaf, sib, par_sib, q1, q2 (else is skipped)"
    );
    assert_eq!(
        finished_count(&log),
        13,
        "seven inner nodes plus the six running leaves each finish once"
    );
    assert_eq!(
        log.last(),
        Some(&LifecycleEvent::Finished("SequenceNode".into(), TaskStatus::Success)),
        "the root sequence resolves last and succeeds"
    );
    assert_eq!(cycles, 2, "The whole tree resolves in 2 cycles becaouse of the LoopUntilNode");
}

#[test]
fn criminally_wide_sequence_tree() {
    // This tree has a sequence with 50 1-tick nodes.
    const WIDTH: usize = 50;

    let mut tree = BehaviourTree::new(SequenceNode);
    let root = tree.get_root_id();
    for i in 0..WIDTH {
        tree.push_child(root, ProbeNode::once(format!("c{i}"), TaskStatus::Success, 1));
    }

    let (log, cycles) = complete_with_cycles(tree);

    assert_eq!(started_count(&log), WIDTH, "every one of the 50 children runs");
    assert_eq!(
        log.last(),
        Some(&LifecycleEvent::Finished("SequenceNode".into(), TaskStatus::Success)),
        "all children succeed, so the sequence succeeds"
    );
    assert_eq!(cycles, WIDTH, "it took {WIDTH} cycles, but was perfored in one update call");
}

#[test]
fn very_undecisive_tree() {
    // This tree has a condition with chain of 50 not nodes.
    const DEPTH: usize = 50;

    let mut tree = BehaviourTree::new(ConditionNode);
    let cond = tree.get_root_id();
    let mut parent = tree.push_child(cond, NotNode);
    for _ in 1..DEPTH {
        parent = tree.push_child(parent, NotNode);
    }
    tree.push_child(parent, ProbeNode::once("gate", TaskStatus::Success, 0));
    tree.push_child(cond, ProbeNode::once("true_branch", TaskStatus::Success, 0));
    tree.push_child(cond, ProbeNode::once("else_branch", TaskStatus::Failure, 0));

    let (log, cycles) = complete_with_cycles(tree);

    assert!(ran(&log, "gate"), "the condition's not-chain evaluates its leaf");
    assert!(
        ran(&log, "true_branch"),
        "not-chain is even, so it doesn't flip condition; the true branch runs"
    );
    assert!(!ran(&log, "else_branch"), "the else branch is never touched");
    assert_eq!(
        finished_count(&log),
        DEPTH + 3,
        "50 nots, the gate leaf, the true branch, and the condition each finish once"
    );
    assert_eq!(
        log.last(),
        Some(&LifecycleEvent::Finished("ConditionNode".into(), TaskStatus::Success)),
        "the condition propagates its true branch's success"
    );
    assert_eq!(cycles, 1, "the whole instant not-chain collapses within one cycle");
}
