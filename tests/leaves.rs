mod common;

use bevy::log::tracing::Level;
use common::*;
use kisya_edbt::prelude::*;

#[test]
fn wait_node() {
    assert_eq!(
        complete_in(BehaviourTree::new(WaitNode::frames(2)), 5),
        lifecycle![WaitNode => success],
        "finishes once its duration elapses"
    );
    assert_eq!(
        TestHarness::new().run_for(BehaviourTree::new(WaitNode::frames(10)), 5).1,
        lifecycle![],
        "doesn't finish until frames/timer run out"
    );
}

#[test]
fn log_node() {
    let (events, records) = capture_logs(|| {
        complete(behaviour_tree! {
            SequenceNode => [
                LogNode::trace("trace message"),
                LogNode::debug("debug message"),
                LogNode::info("info message"),
                LogNode::warn("warn message"),
                LogNode::error("error message")
            ]
        })
    });

    assert_eq!(
        events,
        lifecycle![
            LogNode => success,
            LogNode => success,
            LogNode => success,
            LogNode => success,
            LogNode => success,
            SequenceNode => success,
        ],
        "every log level finishes with Success instantly, without blocking the sequence"
    );
    assert_eq!(
        records.last_chunk::<5>().unwrap(),
        &[
            (Level::TRACE, "trace message".to_string()),
            (Level::DEBUG, "debug message".to_string()),
            (Level::INFO, "info message".to_string()),
            (Level::WARN, "warn message".to_string()),
            (Level::ERROR, "error message".to_string()),
        ],
        "each LogNode actually emits a bevy_log record at its configured level with its message"
    );
}

#[test]
fn subtree_node() {
    let mut harness = TestHarness::new();
    let inner =
        harness.add_tree(BehaviourTree::new(ProbeNode::once("inner", TaskStatus::Failure, 0)));
    assert_eq!(
        harness.run_once(BehaviourTree::new(SubtreeNode::from_handle(inner))),
        (true, lifecycle![+inner, inner => failure, SubtreeNode => failure]),
        "the inner tree runs, and its status propagates"
    );
}
