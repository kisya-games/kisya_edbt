//! App extensions to build custom [`BehaviourNodes`][BehaviourNode].

use std::marker::PhantomData;

use bevy::{
    ecs::{
        lifecycle::Add,
        schedule::ScheduleConfigs,
        system::{IntoObserverSystem, ScheduleSystem},
    },
    prelude::*,
    reflect::GetTypeRegistration,
};

use super::BehaviourTreeNodeLibrary;
use crate::core::{
    node::BehaviourNode,
    runner::{BehaviourUpdateCycleSchedule, BehaviourUpdateCycleSystems},
    task::{ChildTaskFinished, ChildTaskSpawned, TaskFinished, TaskWorker, TaskWorkerPlugin},
};

/// Utility registrar to create new [`BehaviourNodes`][BehaviourNode] safely.
///
/// Node must have at least one system or observer to be valid, and it will be
/// registered only on [`BehaviourNodeAppRegistrar::register`].
pub struct BehaviourNodeAppRegistrar<'app, N>
where
    N: BehaviourNode,
    N::Task: FromReflect,
{
    app: &'app mut App,
    systems: Vec<ScheduleConfigs<ScheduleSystem>>,
    observers: Vec<Box<dyn FnOnce(&mut App)>>,
    _phantom: PhantomData<N>,
}

impl<'app, N> BehaviourNodeAppRegistrar<'app, N>
where
    N: BehaviourNode + GetTypeRegistration + TypePath,
    N::Task: FromReflect + GetTypeRegistration + TypePath,
{
    /// Creates a new empty registrar.
    #[must_use = "Register this node using `register()`"]
    pub fn new(app: &'app mut App) -> Self {
        Self { app, _phantom: PhantomData, systems: vec![], observers: vec![] }
    }

    /// Adds a new per-frame update system to this node.
    #[must_use = "Register this node using `register()`"]
    pub fn with_system<M>(mut self, system: impl IntoScheduleConfigs<ScheduleSystem, M>) -> Self {
        self.systems.push(system.into_configs());
        self
    }

    /// Adds a new setup observer to this node, run once when its task spawns.
    ///
    /// The observer must be typed as `On<Add, TaskWorker<N>>`.
    #[must_use = "Register this node using `register()`"]
    pub fn with_setup_observer<M>(
        mut self,
        observer: impl IntoObserverSystem<Add, TaskWorker<N>, M>,
    ) -> Self {
        self.observers.push(Box::new(move |app| {
            app.add_observer(observer);
        }));
        self
    }

    /// Adds a new [`TaskFinished`] observer to this node, run once when its
    /// task finishes, right before it despawns.
    ///
    /// The observer must be typed as `On<TaskFinished, TaskWorker<N>>`.
    #[must_use = "Register this node using `register()`"]
    pub fn with_finish_observer<M>(
        mut self,
        observer: impl IntoObserverSystem<TaskFinished, TaskWorker<N>, M>,
    ) -> Self {
        self.observers.push(Box::new(move |app| {
            app.add_observer(observer);
        }));
        self
    }

    /// Adds a new [`ChildTaskFinished`] observer to this node.
    ///
    /// The observer is gated by parent's node worker component, so it must be
    /// typed as `On<ChildTaskFinished, TaskWorker<N>>`.
    #[must_use = "Register this node using `register()`"]
    pub fn with_child_finish_observer<M>(
        mut self,
        observer: impl IntoObserverSystem<ChildTaskFinished, TaskWorker<N>, M>,
    ) -> Self {
        self.observers.push(Box::new(move |app| {
            app.add_observer(observer);
        }));
        self
    }

    /// Adds a [`ChildTaskSpawned`] observer to this node.
    ///
    /// The observer is gated by parent's node worker component, so it must be
    /// typed as `On<ChildTaskFinished, TaskWorker<N>>`.
    #[must_use = "Register this node using `register()`"]
    pub fn with_child_spawned_observer<M>(
        mut self,
        observer: impl IntoObserverSystem<ChildTaskSpawned, TaskWorker<N>, M>,
    ) -> Self {
        self.observers.push(Box::new(move |app| {
            app.add_observer(observer);
        }));
        self
    }

    /// Finish registering, validate node configuration, then add everything
    /// to the App.
    pub fn register(mut self) {
        if self.systems.is_empty() && self.observers.is_empty() {
            let t = N::short_type_path();
            panic!(
                "{t} has no systems or observers! Try to add one with `app.add_behaviour_node::<{t}>().with_system(system)` or `.with_setup_observer(observer)`"
            );
        }

        self.app.register_type::<N>();
        self.app.add_plugins(TaskWorkerPlugin::<N>::new());

        for system in self.systems.drain(..) {
            self.app.add_systems(
                BehaviourUpdateCycleSchedule,
                system
                    .run_if(any_with_component::<TaskWorker<N>>)
                    .in_set(BehaviourUpdateCycleSystems::Update),
            );
        }

        for register_observer in self.observers.drain(..) {
            register_observer(self.app);
        }

        self.app.register_type_data::<N, ReflectDefault>();

        self.app
            .world_mut()
            .resource_mut::<BehaviourTreeNodeLibrary>()
            .insert_type_id(std::any::TypeId::of::<N>());

        #[cfg(debug_assertions)]
        {
            use serde::de::DeserializeSeed;

            use crate::core::{
                ser::{BehaviourTreeDeserializer, BehaviourTreeSerializer},
                tree::BehaviourTree,
            };

            let t = N::short_type_path();
            let registry = self.app.world().resource::<AppTypeRegistry>().0.clone();
            let tree = BehaviourTree::new(N::default());
            let serialized = ron::ser::to_string(&BehaviourTreeSerializer::new(&tree, &registry))
                .unwrap_or_else(|err| panic!("{t} cannot be serialized into a tree asset: {err}"));
            let mut deserializer = ron::de::Deserializer::from_str(&serialized)
                .unwrap_or_else(|err| panic!("{t} serialized into invalid RON: {err}"));
            BehaviourTreeDeserializer::new(&registry)
                .deserialize(&mut deserializer)
                .unwrap_or_else(|err| {
                    panic!("{t} cannot be deserialized from a tree asset: {err}")
                });
        }
    }
}

/// Extension trait for `App` to register new nodes.
pub trait BehaviourNodeRegistrarAppExt {
    /// Start registering a new [`BehaviourNode`] in this App.
    fn add_behaviour_node<N>(&mut self) -> BehaviourNodeAppRegistrar<'_, N>
    where
        N: BehaviourNode + GetTypeRegistration + TypePath,
        N::Task: FromReflect + GetTypeRegistration + TypePath;
}

impl BehaviourNodeRegistrarAppExt for App {
    fn add_behaviour_node<N>(&mut self) -> BehaviourNodeAppRegistrar<'_, N>
    where
        N: BehaviourNode + GetTypeRegistration + TypePath,
        N::Task: FromReflect + GetTypeRegistration + TypePath,
    {
        BehaviourNodeAppRegistrar::<N>::new(self)
    }
}
