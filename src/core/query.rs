//! Behaviour tree related [`World`] queries and system params.
//!
//! ## Update systems
//!
//! For usual behaviour systems, prefer [`TaskMut`] (or [`TaskRef`] for
//! read-only): it derefs to the task itself and also carries the task entity,
//! its actor and its source node id, all while automatically skipping sleeping
//! tasks. [`NodeRef`] as query data gives you full information about the node
//! that spawned it.
//!
//! Nothing stops you from querying whatever you want directly
//! (`&TaskWorker<N>`, `&TaskOf`, and so on), but if you do, keep two things in
//! mind:
//! - Tasks can be **sleeping** ([`SleepingTask`]): a raw query will match them
//!   too, so add a `Without<SleepingTask>` filter;
//! - Tasks can be **disabled**
//!   ([`DisabledTask`](crate::core::task::DisabledTask)): like bevy's
//!   `Disabled`, a disabled task is skipped by every query that doesn't
//!   explicitly mention the marker.
//!
//! ```
//! # use kisya_edbt::prelude::*;
//! # use bevy::prelude::*;
//! fn custom_loop_node_system(
//!     mut commands: Commands,
//!     mut query: Query<(TaskMut<LoopNode>, NodeRef<LoopNode>)>,
//! ) {
//!     for (task, node) in &mut query {
//!         // `is_running` is a custom field of this node's task, we can
//!         // easily access it.
//!         if task.is_running {
//!             continue;
//!         }
//!
//!         // Loop is a decorator, and you can access its decorated child node.
//!         let decorated_child = node.info().get_child();
//!
//!         commands.entity(task.entity()).insert(TaskStatus::Failure);
//!     }
//! }
//! ```

use bevy::{
    ecs::{
        archetype::Archetype,
        change_detection::Tick,
        component::{ComponentId, Components},
        query::{
            EcsAccessType, FilteredAccess, FilteredAccessSet, IterQueryData, QueryData,
            ReadOnlyQueryData, WorldQuery,
        },
        storage::{Table, TableRow},
        world::unsafe_world_cell::UnsafeWorldCell,
    },
    prelude::*,
};

use crate::core::{
    node::{BehaviourNode, NodeRef, TreeNodeId},
    task::{SleepingTask, TaskInfo, TaskOf, TaskWorker},
    tree::BehaviourTree,
};

/// [`WorldQuery::Fetch`] for [`NodeRef`].
pub struct NodeRefFetch<'w> {
    node_source: <&'w TaskInfo as WorldQuery>::Fetch<'w>,
    trees: &'w Assets<BehaviourTree>,
}

impl<'w> Clone for NodeRefFetch<'w> {
    fn clone(&self) -> Self { Self { node_source: self.node_source.clone(), trees: self.trees } }
}

/// [`WorldQuery::State`] for [`NodeRef`].
pub struct NodeRefState<N: BehaviourNode> {
    node_source: <&'static TaskInfo as WorldQuery>::State,
    worker: ComponentId,
    trees: ComponentId,
    _phantom: std::marker::PhantomData<N>,
}

impl<N: BehaviourNode> Clone for NodeRefState<N> {
    fn clone(&self) -> Self { *self }
}
impl<N: BehaviourNode> Copy for NodeRefState<N> {}

unsafe impl<'a, N: BehaviourNode> WorldQuery for NodeRef<'a, N> {
    type Fetch<'w> = NodeRefFetch<'w>;
    type State = NodeRefState<N>;

    const IS_DENSE: bool = <&'static TaskInfo as WorldQuery>::IS_DENSE;

    fn shrink_fetch<'wlong: 'wshort, 'wshort>(fetch: Self::Fetch<'wlong>) -> Self::Fetch<'wshort> {
        NodeRefFetch {
            node_source: <&TaskInfo as WorldQuery>::shrink_fetch(fetch.node_source),
            trees: fetch.trees,
        }
    }

    unsafe fn init_fetch<'w, 's>(
        world: UnsafeWorldCell<'w>,
        state: &'s Self::State,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Fetch<'w> {
        NodeRefFetch {
            // SAFETY: caller upholds `init_fetch`'s contract, which covers the component access
            // delegated to `&TaskInfo` below.
            node_source: unsafe {
                <&TaskInfo as WorldQuery>::init_fetch(world, &state.node_source, last_run, this_run)
            },
            // SAFETY: the resource read is declared in `init_nested_access`, which the caller must
            // have already run against this `world`.
            trees: unsafe { world.get_resource::<Assets<BehaviourTree>>() }
                .expect("Assets<BehaviourTree> resource missing"),
        }
    }

    unsafe fn set_archetype<'w, 's>(
        fetch: &mut Self::Fetch<'w>,
        state: &'s Self::State,
        archetype: &'w Archetype,
        table: &'w Table,
    ) {
        // SAFETY: forwarded from the caller.
        unsafe {
            <&TaskInfo as WorldQuery>::set_archetype(
                &mut fetch.node_source,
                &state.node_source,
                archetype,
                table,
            )
        };
    }

    unsafe fn set_table<'w, 's>(
        fetch: &mut Self::Fetch<'w>,
        state: &'s Self::State,
        table: &'w Table,
    ) {
        // SAFETY: forwarded from the caller.
        unsafe {
            <&TaskInfo as WorldQuery>::set_table(&mut fetch.node_source, &state.node_source, table)
        };
    }

    fn update_component_access(state: &Self::State, access: &mut FilteredAccess) {
        <&TaskInfo as WorldQuery>::update_component_access(&state.node_source, access);
        access.and_with(state.worker);
    }

    fn init_nested_access(
        state: &Self::State,
        _system_name: Option<&str>,
        component_access_set: &mut FilteredAccessSet,
        _world: UnsafeWorldCell,
    ) {
        component_access_set.add_resource_read(state.trees);
    }

    fn init_state(world: &mut World) -> Self::State {
        NodeRefState {
            node_source: <&TaskInfo as WorldQuery>::init_state(world),
            worker: world.register_component::<TaskWorker<N>>(),
            trees: world.register_component::<Assets<BehaviourTree>>(),
            _phantom: std::marker::PhantomData,
        }
    }

    fn get_state(components: &Components) -> Option<Self::State> {
        Some(NodeRefState {
            node_source: <&TaskInfo as WorldQuery>::get_state(components)?,
            worker: components.component_id::<TaskWorker<N>>()?,
            trees: components.component_id::<Assets<BehaviourTree>>()?,
            _phantom: std::marker::PhantomData,
        })
    }

    fn matches_component_set(
        state: &Self::State,
        set_contains_id: &impl Fn(ComponentId) -> bool,
    ) -> bool {
        <&TaskInfo as WorldQuery>::matches_component_set(&state.node_source, set_contains_id)
            && set_contains_id(state.worker)
    }
}

// SAFETY: `Self` is its own read-only variant; it never mutates anything.
unsafe impl<'a, N: BehaviourNode> QueryData for NodeRef<'a, N> {
    type Item<'w, 's> = NodeRef<'w, N>;
    type ReadOnly = Self;

    const IS_ARCHETYPAL: bool = true;
    const IS_READ_ONLY: bool = true;

    fn shrink<'wlong: 'wshort, 'wshort, 's>(
        item: Self::Item<'wlong, 's>,
    ) -> Self::Item<'wshort, 's> {
        item
    }

    unsafe fn fetch<'w, 's>(
        state: &'s Self::State,
        fetch: &mut Self::Fetch<'w>,
        entity: Entity,
        table_row: TableRow,
    ) -> Option<Self::Item<'w, 's>> {
        // SAFETY: forwarded from the caller of `QueryData::fetch`.
        let node_source = unsafe {
            <&TaskInfo as QueryData>::fetch(
                &state.node_source,
                &mut fetch.node_source,
                entity,
                table_row,
            )
        }
        .expect("TaskInfo missing while fetching NodeRef<N>");

        let tree =
            fetch.trees.get(node_source.source.tree).expect("NodeRef has invalid associated tree");
        Some(NodeRef::try_new(node_source.source, tree).expect("Invalid node used"))
    }

    fn iter_access(state: &Self::State) -> impl Iterator<Item = EcsAccessType<'_>> {
        <&TaskInfo as QueryData>::iter_access(&state.node_source)
    }
}

// SAFETY: `Self` only ever reads: the delegated `TaskInfo` component and the
// `Assets<BehaviourTree>` resource. It never conflicts across entities.
unsafe impl<'a, N: BehaviourNode> ReadOnlyQueryData for NodeRef<'a, N> {}
// SAFETY: same as above - read-only access is always safe to alias.
unsafe impl<'a, N: BehaviourNode> IterQueryData for NodeRef<'a, N> {}

type TaskRefTuple<'w, N> = (Entity, &'w TaskWorker<N>, &'w TaskOf, &'w TaskInfo);
type TaskMutTuple<'w, N> = (Entity, &'w mut TaskWorker<N>, &'w TaskOf, &'w TaskInfo);

/// Read-only composite view of a task: its own entity, [`TaskWorker`] (deref to
/// the task itself), [`TaskOf`] (the actor it's running for) and [`TaskInfo`]
/// (the node it was spawned from), queried together in one go. The read-only
/// counterpart of [`TaskMut`] (mirroring bevy's [`Ref`]/[`Mut`]).
///
/// Automatically excludes tasks with [`SleepingTask`], it's always safe to use
/// in a per-frame behaviour systems/observers without any extra filters.
pub struct TaskRef<'a, N: BehaviourNode> {
    entity: Entity,
    worker: &'a TaskWorker<N>,
    task_of: &'a TaskOf,
    info: &'a TaskInfo,
}

impl<'a, N: BehaviourNode> TaskRef<'a, N> {
    /// This task's own entity.
    pub fn entity(&self) -> Entity { self.entity }

    /// Actor entity this task is running for.
    pub fn actor(&self) -> Entity { self.task_of.actor() }

    /// Node (and its tree) this task was spawned from.
    pub fn source(&self) -> TreeNodeId { self.info.source }
}

impl<N: BehaviourNode> std::ops::Deref for TaskRef<'_, N> {
    type Target = N::Task;

    fn deref(&self) -> &Self::Target { &self.worker.task }
}

/// Mutable composite view of a task: its own entity, [`TaskWorker`] (deref to
/// the task itself), [`TaskOf`] (the actor it's running for) and [`TaskInfo`]
/// (the node it was spawned from), queried together in one go. The mutable
/// counterpart of [`TaskRef`] (mirroring bevy's [`Ref`]/[`Mut`]).
///
/// Automatically excludes tasks with [`SleepingTask`], it's always safe to use
/// in a per-frame behaviour systems/observers without any extra filters.
/// behaviour system query without an extra filter.
pub struct TaskMut<'a, N: BehaviourNode> {
    entity: Entity,
    worker: Mut<'a, TaskWorker<N>>,
    task_of: &'a TaskOf,
    info: &'a TaskInfo,
}

impl<'a, N: BehaviourNode> TaskMut<'a, N> {
    /// This task's own entity.
    pub fn entity(&self) -> Entity { self.entity }

    /// Actor entity this task is running for.
    pub fn actor(&self) -> Entity { self.task_of.actor() }

    /// Node (and its tree) this task was spawned from.
    pub fn source(&self) -> TreeNodeId { self.info.source }
}

impl<N: BehaviourNode> std::ops::Deref for TaskMut<'_, N> {
    type Target = N::Task;

    fn deref(&self) -> &Self::Target { &self.worker.task }
}

impl<N: BehaviourNode> std::ops::DerefMut for TaskMut<'_, N> {
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.worker.task }
}

/// [`WorldQuery::State`] shared by [`TaskRef`] and [`TaskMut`].
pub struct TaskViewState<N: BehaviourNode> {
    tuple: <TaskRefTuple<'static, N> as WorldQuery>::State,
    sleeping: ComponentId,
}

impl<N: BehaviourNode> Clone for TaskViewState<N> {
    fn clone(&self) -> Self { *self }
}
impl<N: BehaviourNode> Copy for TaskViewState<N> {}

// SAFETY: component access is entirely delegated to the underlying tuple's own
// implementation.
unsafe impl<'a, N: BehaviourNode> WorldQuery for TaskRef<'a, N> {
    type Fetch<'w> = <TaskRefTuple<'w, N> as WorldQuery>::Fetch<'w>;
    type State = TaskViewState<N>;

    const IS_DENSE: bool = <TaskRefTuple<'static, N> as WorldQuery>::IS_DENSE;

    fn shrink_fetch<'wlong: 'wshort, 'wshort>(fetch: Self::Fetch<'wlong>) -> Self::Fetch<'wshort> {
        <TaskRefTuple<N> as WorldQuery>::shrink_fetch(fetch)
    }

    unsafe fn init_fetch<'w, 's>(
        world: UnsafeWorldCell<'w>,
        state: &'s Self::State,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Fetch<'w> {
        // SAFETY: forwarded from the caller.
        unsafe {
            <TaskRefTuple<N> as WorldQuery>::init_fetch(world, &state.tuple, last_run, this_run)
        }
    }

    unsafe fn set_archetype<'w, 's>(
        fetch: &mut Self::Fetch<'w>,
        state: &'s Self::State,
        archetype: &'w Archetype,
        table: &'w Table,
    ) {
        // SAFETY: forwarded from the caller.
        unsafe {
            <TaskRefTuple<N> as WorldQuery>::set_archetype(fetch, &state.tuple, archetype, table)
        };
    }

    unsafe fn set_table<'w, 's>(
        fetch: &mut Self::Fetch<'w>,
        state: &'s Self::State,
        table: &'w Table,
    ) {
        // SAFETY: forwarded from the caller.
        unsafe { <TaskRefTuple<N> as WorldQuery>::set_table(fetch, &state.tuple, table) };
    }

    fn update_component_access(state: &Self::State, access: &mut FilteredAccess) {
        <TaskRefTuple<N> as WorldQuery>::update_component_access(&state.tuple, access);
        access.and_without(state.sleeping);
    }

    fn init_nested_access(
        state: &Self::State,
        system_name: Option<&str>,
        component_access_set: &mut FilteredAccessSet,
        world: UnsafeWorldCell,
    ) {
        <TaskRefTuple<N> as WorldQuery>::init_nested_access(
            &state.tuple,
            system_name,
            component_access_set,
            world,
        );
    }

    fn init_state(world: &mut World) -> Self::State {
        TaskViewState {
            tuple: <TaskRefTuple<N> as WorldQuery>::init_state(world),
            sleeping: world.register_component::<SleepingTask>(),
        }
    }

    fn get_state(components: &Components) -> Option<Self::State> {
        Some(TaskViewState {
            tuple: <TaskRefTuple<N> as WorldQuery>::get_state(components)?,
            sleeping: components.component_id::<SleepingTask>()?,
        })
    }

    fn matches_component_set(
        state: &Self::State,
        set_contains_id: &impl Fn(ComponentId) -> bool,
    ) -> bool {
        <TaskRefTuple<N> as WorldQuery>::matches_component_set(&state.tuple, set_contains_id)
            && !set_contains_id(state.sleeping)
    }
}

// SAFETY: `Self` is its own read-only variant; it never mutates anything.
unsafe impl<'a, N: BehaviourNode> QueryData for TaskRef<'a, N> {
    type Item<'w, 's> = TaskRef<'w, N>;
    type ReadOnly = Self;

    const IS_ARCHETYPAL: bool = <TaskRefTuple<'static, N> as QueryData>::IS_ARCHETYPAL;
    const IS_READ_ONLY: bool = true;

    fn shrink<'wlong: 'wshort, 'wshort, 's>(
        item: Self::Item<'wlong, 's>,
    ) -> Self::Item<'wshort, 's> {
        item
    }

    unsafe fn fetch<'w, 's>(
        state: &'s Self::State,
        fetch: &mut Self::Fetch<'w>,
        entity: Entity,
        table_row: TableRow,
    ) -> Option<Self::Item<'w, 's>> {
        // SAFETY: forwarded from the caller of `QueryData::fetch`.
        let (entity, worker, task_of, info) = unsafe {
            <TaskRefTuple<N> as QueryData>::fetch(&state.tuple, fetch, entity, table_row)
        }?;
        Some(TaskRef { entity, worker, task_of, info })
    }

    fn iter_access(state: &Self::State) -> impl Iterator<Item = EcsAccessType<'_>> {
        <TaskRefTuple<N> as QueryData>::iter_access(&state.tuple)
    }
}

// SAFETY: `Self` only ever reads components local to the current entity.
unsafe impl<'a, N: BehaviourNode> ReadOnlyQueryData for TaskRef<'a, N> {}
// SAFETY: same as above - read-only access is always safe to alias.
unsafe impl<'a, N: BehaviourNode> IterQueryData for TaskRef<'a, N> {}

// SAFETY: component access is entirely delegated to the underlying tuple's own
// implementation.
unsafe impl<'a, N: BehaviourNode> WorldQuery for TaskMut<'a, N> {
    type Fetch<'w> = <TaskMutTuple<'w, N> as WorldQuery>::Fetch<'w>;
    type State = TaskViewState<N>;

    const IS_DENSE: bool = <TaskMutTuple<'static, N> as WorldQuery>::IS_DENSE;

    fn shrink_fetch<'wlong: 'wshort, 'wshort>(fetch: Self::Fetch<'wlong>) -> Self::Fetch<'wshort> {
        <TaskMutTuple<N> as WorldQuery>::shrink_fetch(fetch)
    }

    unsafe fn init_fetch<'w, 's>(
        world: UnsafeWorldCell<'w>,
        state: &'s Self::State,
        last_run: Tick,
        this_run: Tick,
    ) -> Self::Fetch<'w> {
        // SAFETY: forwarded from the caller.
        unsafe {
            <TaskMutTuple<N> as WorldQuery>::init_fetch(world, &state.tuple, last_run, this_run)
        }
    }

    unsafe fn set_archetype<'w, 's>(
        fetch: &mut Self::Fetch<'w>,
        state: &'s Self::State,
        archetype: &'w Archetype,
        table: &'w Table,
    ) {
        // SAFETY: forwarded from the caller.
        unsafe {
            <TaskMutTuple<N> as WorldQuery>::set_archetype(fetch, &state.tuple, archetype, table)
        };
    }

    unsafe fn set_table<'w, 's>(
        fetch: &mut Self::Fetch<'w>,
        state: &'s Self::State,
        table: &'w Table,
    ) {
        // SAFETY: forwarded from the caller.
        unsafe { <TaskMutTuple<N> as WorldQuery>::set_table(fetch, &state.tuple, table) };
    }

    fn update_component_access(state: &Self::State, access: &mut FilteredAccess) {
        <TaskMutTuple<N> as WorldQuery>::update_component_access(&state.tuple, access);
        access.and_without(state.sleeping);
    }

    fn init_nested_access(
        state: &Self::State,
        system_name: Option<&str>,
        component_access_set: &mut FilteredAccessSet,
        world: UnsafeWorldCell,
    ) {
        <TaskMutTuple<N> as WorldQuery>::init_nested_access(
            &state.tuple,
            system_name,
            component_access_set,
            world,
        );
    }

    fn init_state(world: &mut World) -> Self::State {
        TaskViewState {
            tuple: <TaskMutTuple<N> as WorldQuery>::init_state(world),
            sleeping: world.register_component::<SleepingTask>(),
        }
    }

    fn get_state(components: &Components) -> Option<Self::State> {
        Some(TaskViewState {
            tuple: <TaskMutTuple<N> as WorldQuery>::get_state(components)?,
            sleeping: components.component_id::<SleepingTask>()?,
        })
    }

    fn matches_component_set(
        state: &Self::State,
        set_contains_id: &impl Fn(ComponentId) -> bool,
    ) -> bool {
        <TaskMutTuple<N> as WorldQuery>::matches_component_set(&state.tuple, set_contains_id)
            && !set_contains_id(state.sleeping)
    }
}

// SAFETY: `TaskRef<N>` only accesses a subset (immutable) of what `Self`
// accesses, and matches the same archetypes/tables.
unsafe impl<'a, N: BehaviourNode> QueryData for TaskMut<'a, N> {
    type Item<'w, 's> = TaskMut<'w, N>;
    type ReadOnly = TaskRef<'a, N>;

    const IS_ARCHETYPAL: bool = <TaskMutTuple<'static, N> as QueryData>::IS_ARCHETYPAL;
    const IS_READ_ONLY: bool = false;

    fn shrink<'wlong: 'wshort, 'wshort, 's>(
        item: Self::Item<'wlong, 's>,
    ) -> Self::Item<'wshort, 's> {
        item
    }

    unsafe fn fetch<'w, 's>(
        state: &'s Self::State,
        fetch: &mut Self::Fetch<'w>,
        entity: Entity,
        table_row: TableRow,
    ) -> Option<Self::Item<'w, 's>> {
        // SAFETY: forwarded from the caller of `QueryData::fetch`.
        let (entity, worker, task_of, info) = unsafe {
            <TaskMutTuple<N> as QueryData>::fetch(&state.tuple, fetch, entity, table_row)
        }?;
        Some(TaskMut { entity, worker, task_of, info })
    }

    fn iter_access(state: &Self::State) -> impl Iterator<Item = EcsAccessType<'_>> {
        <TaskMutTuple<N> as QueryData>::iter_access(&state.tuple)
    }
}

// SAFETY: only ever accesses `TaskWorker<N>` (uniquely) on the current
// entity; `TaskOf`/`TaskInfo` reads never conflict across entities.
unsafe impl<'a, N: BehaviourNode> IterQueryData for TaskMut<'a, N> {}
