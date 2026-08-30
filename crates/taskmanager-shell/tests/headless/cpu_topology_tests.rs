//! Deterministic CPU-topology fixture invariants (shell fixture boundary).

use taskmanager_core::core::hardware::CpuType;

use super::{CpuClusterSpec, CpuTopologySpec, demo_cpu_topology};

fn cluster(kind: CpuType, physical_cores: usize, threads_per_core: usize) -> CpuClusterSpec {
    CpuClusterSpec {
        kind,
        physical_cores,
        threads_per_core,
    }
}

/// The market's real shapes must all be expressible as free cluster
/// composition — and every composition must generate self-consistent
/// seeds (all per-core vectors exactly `logical_cores` long, types
/// agreeing with the cluster arithmetic, declared counts derived).
/// Covers: Intel hybrid 3-cluster, homogeneous AMD/server with SMT on
/// EVERY core (one cluster), Snapdragon-style 1X+5P+2LP three-cluster,
/// Apple-style two-cluster, and a FOUR-cluster shape (the model has no
/// cluster-count ceiling).
#[test]
fn market_cpu_shapes_are_expressible_and_self_consistent() {
    let shapes: Vec<(&str, CpuTopologySpec, usize, usize)> = vec![
        (
            "Intel Ultra 7 358H (P+SMT, E, LP-E)",
            CpuTopologySpec {
                clusters: vec![
                    cluster(CpuType::Performance, 6, 2),
                    cluster(CpuType::Efficient, 8, 1),
                    cluster(CpuType::LowPower, 2, 1),
                ],
            },
            16,
            22,
        ),
        (
            "AMD Ryzen 9 7950X (homogeneous, SMT on EVERY core)",
            CpuTopologySpec {
                clusters: vec![cluster(CpuType::Performance, 16, 2)],
            },
            16,
            32,
        ),
        (
            "Snapdragon-style 1X+5P+2LP three-cluster",
            CpuTopologySpec {
                clusters: vec![
                    cluster(CpuType::Performance, 1, 1),
                    cluster(CpuType::Performance, 5, 1),
                    cluster(CpuType::Efficient, 2, 1),
                ],
            },
            8,
            8,
        ),
        (
            "Apple-style big.LITTLE two-cluster",
            CpuTopologySpec {
                clusters: vec![
                    cluster(CpuType::Performance, 4, 1),
                    cluster(CpuType::Efficient, 4, 1),
                ],
            },
            8,
            8,
        ),
        (
            "four clusters (no model ceiling)",
            CpuTopologySpec {
                clusters: vec![
                    cluster(CpuType::Performance, 2, 2),
                    cluster(CpuType::Performance, 4, 1),
                    cluster(CpuType::Efficient, 4, 1),
                    cluster(CpuType::LowPower, 2, 1),
                ],
            },
            12,
            14,
        ),
    ];

    for (name, topology, physical, logical) in shapes {
        assert_eq!(topology.physical_cores(), physical, "{name}");
        assert_eq!(topology.logical_cores(), logical, "{name}");
        assert_eq!(topology.core_usage().len(), logical, "{name}");
        assert_eq!(topology.frequencies_mhz().len(), logical, "{name}");
        assert_eq!(topology.temperatures_c().len(), logical, "{name}");
        assert_eq!(topology.cpu_types().len(), logical, "{name}");
        // SMT factor is per-cluster: a cluster with 2 threads paints two
        // logical CPUs of its kind for every physical core.
        for cluster in &topology.clusters {
            let painted = topology
                .cpu_types()
                .iter()
                .filter(|kind| **kind == cluster.kind)
                .count();
            assert!(
                painted >= cluster.logical_cpus(),
                "{name}: cluster {cluster:?} must fit inside the painted types"
            );
        }
    }
}

/// SMT is per-cluster and NOT limited to P-cores: an Efficiency cluster
/// with `threads_per_core = 2` (a hypothetical AMD-style homogeneous
/// efficiency part, or any future shape) paints two logical CPUs per
/// physical core.
#[test]
fn any_cluster_may_carry_smt() {
    let topology = CpuTopologySpec {
        clusters: vec![cluster(CpuType::Efficient, 4, 2)],
    };
    assert_eq!(topology.logical_cores(), 8);
    let usage = topology.core_usage();
    // The kind's pattern tiles across BOTH threads of the first physical
    // core (18.0 / 25.5), proving per-thread values instead of a copied
    // pair.
    assert_eq!(usage[..2], [18.0, 25.5]);
}

/// The demo profile keeps reproducing the original hand-written demo
/// values byte-for-byte (16 physical / 22 logical, C00 = 52% · 4.82 GHz ·
/// 58 °C), so evidence captures stay comparable across the refactor.
#[test]
fn demo_profile_preserves_the_original_seed_values() {
    let demo = demo_cpu_topology();
    assert_eq!(demo.physical_cores(), 16);
    assert_eq!(demo.logical_cores(), 22);
    let usage = demo.core_usage();
    assert_eq!(usage[..4], [52.0, 41.0, 34.0, 22.0]);
    let frequencies = demo.frequencies_mhz();
    assert_eq!(frequencies[0], 4_820);
    let temperatures = demo.temperatures_c();
    assert_eq!(temperatures[0], 58.0);
}
