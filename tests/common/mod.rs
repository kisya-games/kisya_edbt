#![allow(dead_code)]

use std::{
    borrow::Cow,
    fmt,
    sync::{Arc, Mutex},
    time::Duration,
};

use bevy::{
    asset::AssetPlugin,
    log::{
        tracing::{self, Level, Subscriber},
        tracing_subscriber::{self, Layer, Registry, layer::SubscriberExt},
    },
    prelude::*,
    time::TimeUpdateStrategy,
};
use kisya_edbt::{
    BehaviourPlugins,
    core::{
        runner::BehaviourRunnerCycles,
        task::{DisabledTask, TaskInfo},
    },
    prelude::*,
};
use smallvec::SmallVec;

/// Get all of the logs emitted by `bevy_log` for this run.
pub fn capture_logs<T>(f: impl FnOnce() -> T) -> (T, Vec<(Level, String)>) {
    let records = Arc::new(Mutex::new(Vec::new()));
    let subscriber = Registry::default().with(LogRecorder(records.clone()));
    let result = tracing::subscriber::with_default(subscriber, f);
    (result, records.lock().unwrap().clone())
}

struct LogRecorder(Arc<Mutex<Vec<(Level, String)>>>);

impl<S: Subscriber> Layer<S> for LogRecorder {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut message = String::new();
        event.record(&mut MessageVisitor(&mut message));
        self.0.lock().unwrap().push((*event.metadata().level(), message));
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{value:?}");
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleEvent {
    Started(Cow<'static, str>),
    Ticked(Cow<'static, str>),
    Finished(Cow<'static, str>, TaskStatus),
}

/// Builds a `Vec<ExecEvent>` from a terse lifecycle description, e.g.
/// `lifecycle![+a, ~a, a => success, +b, b => failure]`.
#[macro_export]
macro_rules! lifecycle {
    (@push $events:ident; ) => {};
    (@push $events:ident; + $label:ident $(, $($rest:tt)*)?) => {
        $events.push(LifecycleEvent::Started(stringify!($label).into()));
        lifecycle!(@push $events; $($($rest)*)?);
    };
    (@push $events:ident; ~ $label:ident $(, $($rest:tt)*)?) => {
        $events.push(LifecycleEvent::Ticked(stringify!($label).into()));
        lifecycle!(@push $events; $($($rest)*)?);
    };
    (@push $events:ident; $label:ident => success $(, $($rest:tt)*)?) => {
        $events.push(LifecycleEvent::Finished(stringify!($label).into(), TaskStatus::Success));
        lifecycle!(@push $events; $($($rest)*)?);
    };
    (@push $events:ident; $label:ident => failure $(, $($rest:tt)*)?) => {
        $events.push(LifecycleEvent::Finished(stringify!($label).into(), TaskStatus::Failure));
        lifecycle!(@push $events; $($($rest)*)?);
    };
    ($($rest:tt)*) => {{
        #[allow(unused_mut)]
        let mut events: Vec<LifecycleEvent> = Vec::new();
        lifecycle!(@push events; $($rest)*);
        events
    }};
}

#[derive(Resource, Default)]
struct Lifecycle(Vec<LifecycleEvent>);

/// Scripted test leaf: logs its lifecycle and finishes with a per-run result.
#[derive(Debug, Default, Reflect, Clone)]
pub struct ProbeNode {
    pub label: String,
    /// Result per run; the last one repeats. Empty means `Success`.
    pub results: Vec<TaskStatus>,
    /// 0 finishes instantly in setup, N finishes after N update ticks.
    pub ticks: u32,
}

impl ProbeNode {
    pub fn once(label: impl ToString, result: TaskStatus, ticks: u32) -> ProbeNode {
        ProbeNode { label: label.to_string(), results: vec![result], ticks }
    }

    pub fn serial(label: impl ToString, results: Vec<TaskStatus>, ticks: u32) -> ProbeNode {
        ProbeNode { label: label.to_string(), results, ticks }
    }
}

impl BehaviourNode for ProbeNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ProbeTask;

    fn build_task(&self) -> Self::Task {
        Self::Task { remaining: self.ticks, result: TaskStatus::Running }
    }
}

#[derive(Reflect, Debug, Default)]
pub struct ProbeTask {
    remaining: u32,
    result: TaskStatus,
}

struct ProbeNodePlugin;

impl Plugin for ProbeNodePlugin {
    fn build(&self, app: &mut App) {
        app.add_behaviour_node::<ProbeNode>()
            .with_setup_observer(on_probe_setup_hook)
            .with_system(probe_node_system)
            .register();
    }
}

fn on_probe_setup_hook(
    event: On<Add, TaskWorker<ProbeNode>>,
    mut commands: Commands,
    mut log: ResMut<Lifecycle>,
    mut query: Query<(&mut TaskWorker<ProbeNode>, NodeRef<ProbeNode>)>,
) {
    let Ok((mut worker, node)) = query.get_mut(event.entity) else {
        return;
    };

    let run_index = log
        .0
        .iter()
        .filter(|entry| matches!(entry, LifecycleEvent::Started(label) if *label == node.label))
        .count();
    log.0.push(LifecycleEvent::Started(node.label.clone().into()));

    let result =
        node.results.get(run_index).or(node.results.last()).copied().unwrap_or(TaskStatus::Success);
    if node.ticks == 0 {
        commands.entity(event.entity).insert(result);
    } else {
        worker.result = result;
    }
}

fn probe_node_system(
    mut commands: Commands,
    mut log: ResMut<Lifecycle>,
    mut query: Query<(TaskMut<ProbeNode>, NodeRef<ProbeNode>)>,
) {
    for (mut task, node) in &mut query {
        // An instant probe finished in its setup and only awaits teardown.
        if task.remaining == 0 {
            continue;
        }

        log.0.push(LifecycleEvent::Ticked(node.label.clone().into()));

        task.remaining -= 1;
        if task.remaining == 0 {
            let result = task.result;
            commands.entity(task.entity()).insert(result);
        }
    }
}

fn record_finished_hook(
    event: On<TaskFinished>,
    q_name: Query<&Name>,
    q_node_source: Query<&TaskInfo>,
    trees: Res<Assets<BehaviourTree>>,
    mut log: ResMut<Lifecycle>,
) {
    let name = q_node_source
        .get(event.task)
        .ok()
        .and_then(|info| trees.get(info.source().tree).map(|tree| (info.source(), tree)))
        .and_then(|(node_id, tree)| NodeRef::<ProbeNode>::try_new(node_id, tree))
        .map(|node| node.label.clone().into())
        .or_else(|| q_name.get(event.task).ok().map(|name| name.to_string().into()))
        .unwrap_or_default();
    log.0.push(LifecycleEvent::Finished(name, event.status));
}

pub struct TestHarness {
    app: App,
    actor: Option<Entity>,
}

impl TestHarness {
    pub fn new() -> Self {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), BehaviourPlugins))
            .add_plugins(ProbeNodePlugin)
            .insert_resource(Lifecycle::default())
            .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_millis(100)))
            .add_observer(record_finished_hook);

        Self { app, actor: None }
    }

    pub fn add_tree(&mut self, tree: BehaviourTree) -> Handle<BehaviourTree> {
        self.app.world_mut().resource_mut::<Assets<BehaviourTree>>().add(tree)
    }

    /// Spawn an actor running `tree`, without driving it to completion.
    pub fn spawn(&mut self, tree: BehaviourTree) -> Entity {
        let tree = self.add_tree(tree);
        let actor = self.app.world_mut().spawn(Behaviour { tree }).id();
        self.actor = Some(actor);
        actor
    }

    pub fn update(&mut self) { self.app.update(); }

    pub fn world(&self) -> &World { self.app.world() }

    pub fn world_mut(&mut self) -> &mut World { self.app.world_mut() }

    pub fn run_once(&mut self, tree: BehaviourTree) -> (bool, Vec<LifecycleEvent>) {
        self.run_for(tree, 1)
    }

    pub fn cycles(&self) -> usize { self.world().resource::<BehaviourRunnerCycles>().previous() }

    pub fn tasks(&self) -> SmallVec<[Entity; 4]> {
        let mut q = self.world().try_query_filtered::<Entity, With<TaskStatus>>().unwrap();
        q.iter(self.world()).collect()
    }

    pub fn task_count(&self) -> (usize, usize) {
        let mut q = self.world().try_query::<(&TaskStatus, Has<DisabledTask>)>().unwrap();
        q.iter(self.world()).fold((0, 0), |(total, disabled), (_, is_disabled)| {
            (total + 1, disabled + is_disabled as usize)
        })
    }

    pub fn run_for(&mut self, tree: BehaviourTree, updates: usize) -> (bool, Vec<LifecycleEvent>) {
        let tree = self.add_tree(tree);
        self.actor = Some(self.app.world_mut().spawn(Behaviour { tree }).id());
        for _ in 0..updates {
            self.app.update();
        }
        (self.is_complete(), self.app.world_mut().remove_resource::<Lifecycle>().unwrap().0)
    }

    pub fn is_complete(&self) -> bool {
        let actor = self.actor.expect("no tree was run");
        self.app.world().get::<Behaviour>(actor).is_none()
    }
}

#[inline]
pub fn complete(tree: BehaviourTree) -> Vec<LifecycleEvent> {
    let mut harness = TestHarness::new();
    let (is_complete, lifecycle) = harness.run_once(tree);
    assert!(is_complete);
    lifecycle
}

#[inline]
pub fn complete_with_cycles(tree: BehaviourTree) -> (Vec<LifecycleEvent>, usize) {
    let mut harness = TestHarness::new();
    let (is_complete, lifecycle) = harness.run_once(tree);
    assert!(is_complete);
    (lifecycle, harness.cycles())
}

#[inline]
pub fn complete_in(tree: BehaviourTree, updates: usize) -> Vec<LifecycleEvent> {
    let mut harness = TestHarness::new();
    let (is_complete, lifecycle) = harness.run_for(tree, updates);
    assert!(is_complete);
    lifecycle
}
