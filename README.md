<div align="center">

<img src="assets/logo.png" alt="logo" width="400">

# Kisya EDBT

[![License](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/kisya-games/kisya_edbt?tab=readme-ov-file#license)
[![Crates.io](https://img.shields.io/crates/v/kisya_edbt.svg)](https://crates.io/crates/kisya_edbt)
[![Docs](https://docs.rs/kisya_edbt/badge.svg)](https://docs.rs/kisya_edbt/)
[![CI](https://github.com/kisya-games/kisya_edbt/actions/workflows/rust.yml/badge.svg?branch=main&event=push)](https://github.com/kisya-games/kisya_edbt/actions/workflows/rust.yml)

🌳 **Event-Driven Behaviour Trees for [Bevy](https://bevyengine.org/) game engine** 🕊️

</div>

https://github.com/user-attachments/assets/426e94e4-9707-45c0-ae87-673f9652529c

---

## Features

- **Built upon bevy's ECS**: Tasks are just entities with shared node's info, updated by systems in a custom EDBT schedule.
- **Fine-grained and ergonomic**: Build your custom nodes using observers and/or update systems with custom *TaskRef*/*TaskMut*/*NodeRef* query params.
- **Composable Behaviour Trees**: Create BTs using a small DSL `behaviour_tree!` or just load them from disk as RON assets. Hot reloading and validation included!
- **Common nodes library**: *LoopNode*, *ParallelNode*, *SequenceNode*, *ConditionNode*, *WaitNode* and more. Fully tested, too!
- **Fully documented**: All public API is fully documented, enforced by `#![warn(missing_docs)]`.
- **Builtin Egui editor**: You can edit any *BehaviourTree* within egui once you enable `features = ["egui"]`. See it in the [**purrtle**](examples/purrtle.rs) example (`cargo run --example purrtle --features egui`).

### Planned features

> [!NOTE]  
> 😼 Internally, this library was in use since bevy 0.14. But it still lacks some features and optimization, PRs are welcome!

- [ ] Actual event-driven nodes (hehe)
- [ ] Better performance
- [ ] More common nodes from Game AI Pro
- [ ] Jackdaw-based tooling instead of custom egui ? Maybe ?
- [ ] `kisya_edbt_derive` to generate tasks/node info
- [ ] `Behaviour` and task tree serialization

## Usage

### Adding behaviours to actors

After adding `BehaviourPlugins`, you'll be able to add `Behaviour`s to your actors (any entity, such as an NPC).
To do this, create a new `BehaviourTree` asset, add it to your actor, and that's it.

```rust
use bevy::prelude::*;
use kisya_edbt::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, BehaviourPlugins))
        .add_systems(Startup, setup_system)
        .run();
}

fn setup_system(
    mut commands: Commands,
    mut trees: ResMut<Assets<BehaviourTree>>,
) {
    let tree = trees.add(behaviour_tree! {
        LoopNode => [SequenceNode => [WaitNode::time(2.0), LogNode::info("Miao")]]
    });

    commands.spawn(Behaviour { tree });
}
```

### Creating custom nodes

Of course, you can create your own nodes. Those are split into a type that implements `BehaviourNode` and its associated task.
`BehaviourNode` is the static part of the node -- shared across all tasks. Tasks are the actual workhorses, driving behaviour.
To learn more, see how builtin nodes are implemented.

```rust
use bevy::prelude::*;
use kisya_edbt::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, BehaviourPlugins));
    app.add_behaviour_node::<MoveNode>().with_system(move_node_system).register();
    app.run();
}

#[derive(Reflect, Default)]
struct MoveNode {
    velocity: Vec3
}

impl BehaviourNode for MoveNode {
    type Info<'a> = LeafNodeInfo<'a>;
    type Task = ();
    fn build_task(&self) -> Self::Task { default() }
}

fn move_node_system(
    mut cmd: Commands,
    time: Res<Time>,
    mut q_task: Query<(TaskRef<MoveNode>, NodeRef<MoveNode>)>,
    mut q_actor: Query<&mut Transform>
) {
    for (task, node) in &mut q_task {
        let Ok(mut transform) = q_actor.get_mut(task.actor()) else {
            cmd.entity(task.entity()).insert(TaskStatus::Failure);
            continue;
        };

        transform.translation += node.velocity * time.delta();
    }
}
```

## Performance

Per-actor cost and total update cost is measured in [**runner_scaling**](benches/runner_scaling.rs) benchmark.
It's probably not a very valid benchmark, because I have no idea what I'm doing. But still:

| scenario | actors | total | per actor |
|---|---:|---:|---:|
| _instant_ | 10 | 0.114 ms | 11.362 µs |
| _instant_ | 100 | 0.287 ms | 2.874 µs |
| _instant_ | 1000 | 2.214 ms | 2.214 µs |
| _instant_ | 10000 | 19.890 ms | 1.989 µs |
| _instant_ | 100000 | 212.244 ms | 2.122 µs |
| _multiframe_ | 10 | 0.080 ms | 7.986 µs |
| _multiframe_ | 100 | 0.089 ms | 0.886 µs |
| _multiframe_ | 1000 | 0.128 ms | 0.128 µs |
| _multiframe_ | 10000 | 0.475 ms | 0.047 µs |
| _multiframe_ | 100000 | 6.919 ms | 0.069 µs |

There are two scenarios: instant for one-shot nodes, and multiframe for nodes updating for a while.
Measured on *12th Gen Intel(R) Core(TM) i7-12800H*.

## Supported Bevy Versions

| Bevy    | Kisya EDBT |
| ------- | ---------- |
| 0.19    | 0.1        |

## License

Licensed under
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)
