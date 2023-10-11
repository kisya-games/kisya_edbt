use bevy::{asset::AssetPlugin, prelude::*};
use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use kisya_edbt::prelude::*;

/// Entity counts to sweep.
const SIZES: [usize; 5] = [10, 100, 1_000, 10_000, 100_000];

/// Spawn N perpetually-running behaviours and let criterion time a single
/// `app.update()`.
///
/// Both scenarios share one group so criterion's summary report plots their
/// per-actor throughput on the same chart, and `Throughput::Elements` reports
/// the per-actor rate directly.
fn update_duration_per_actor(c: &mut Criterion) {
    let scenarios: [(&str, fn() -> BehaviourTree); 2] = [
        ("instant", || behaviour_tree! { LoopNode => [ConstNode::success()] }),
        ("multiframe", || behaviour_tree! { LoopNode => [WaitNode::frames(250)] }),
    ];

    let mut group = c.benchmark_group("update_duration_per_actor");
    group.sample_size(10);

    for (scenario, build) in scenarios {
        for n in SIZES {
            group.throughput(Throughput::Elements(n as u64));
            group.bench_with_input(BenchmarkId::new(scenario, n), &n, |b, &n| {
                let mut app = App::new();
                app.add_plugins((MinimalPlugins, AssetPlugin::default(), BehaviourPlugins));
                let tree = app.world_mut().resource_mut::<Assets<BehaviourTree>>().add(build());
                app.world_mut().spawn_batch((0..n).map(|_| Behaviour { tree: tree.clone() }));

                b.iter(|| app.update());
            });
        }
    }

    group.finish();
}

criterion_group!(benches, update_duration_per_actor);
criterion_main!(benches);
