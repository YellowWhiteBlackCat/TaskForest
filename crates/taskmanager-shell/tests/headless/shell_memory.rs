use super::*;
use taskmanager_application::{
    MemoryCompositionObservations, MemoryCompressionObservations, MemoryOptionalObservations,
    MemoryScalarObservations, OptionalObservation, ScalarObservation,
};

fn memory(
    total: u64,
    used: u64,
    swap: Option<(u64, u64)>,
    composition: MemoryCompositionObservations,
) -> MemoryMetrics {
    MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(total, 1),
            used_bytes: ScalarObservation::available(used, 1),
            available_bytes: ScalarObservation::available(total.saturating_sub(used), 1),
            swap_total_bytes: swap.map_or_else(ScalarObservation::default, |(total, _)| {
                ScalarObservation::available(total, 1)
            }),
            swap_used_bytes: swap.map_or_else(ScalarObservation::default, |(_, used)| {
                ScalarObservation::available(used, 1)
            }),
            ..Default::default()
        },
        MemoryOptionalObservations {
            composition,
            ..Default::default()
        },
    )
}

#[test]
fn partial_composition_falls_back_to_three_segments_not_zero() {
    let memory = memory(
        100,
        40,
        None,
        MemoryCompositionObservations {
            cached_bytes: OptionalObservation::present(10, 1),
            buffers_bytes: OptionalObservation::present(5, 1),
            active_bytes: OptionalObservation::present(20, 1),
            ..Default::default()
        },
    );
    assert_eq!(memory_segments(&memory).len(), 3);
}

#[test]
fn complete_measured_zero_is_distinct_from_unknown() {
    let measured = memory(
        100,
        0,
        None,
        MemoryCompositionObservations {
            buffers_bytes: OptionalObservation::present(0, 1),
            active_bytes: OptionalObservation::present(0, 1),
            inactive_bytes: OptionalObservation::present(0, 1),
            free_bytes: OptionalObservation::present(0, 1),
            reclaimable_bytes: OptionalObservation::present(0, 1),
            ..Default::default()
        },
    );
    let unknown = memory(100, 0, None, Default::default());
    assert_eq!(memory_segments(&measured).len(), 5);
    assert_eq!(memory_segments(&unknown).len(), 2);
}

#[test]
fn full_composition_segments_sum_to_total() {
    let memory = memory(
        1_000,
        400,
        None,
        MemoryCompositionObservations {
            buffers_bytes: OptionalObservation::present(50, 1),
            active_bytes: OptionalObservation::present(300, 1),
            inactive_bytes: OptionalObservation::present(200, 1),
            free_bytes: OptionalObservation::present(150, 1),
            reclaimable_bytes: OptionalObservation::present(50, 1),
            ..Default::default()
        },
    );
    let segments = memory_segments(&memory);
    assert_eq!(segments.len(), 5);
    assert_eq!(
        segments.iter().map(|segment| segment.bytes).sum::<u64>(),
        1_000
    );
    assert_eq!(segments[2].bytes, 100);
    assert_eq!(segments[2].kind, MemSegmentKind::Cache);
    assert_eq!(segments[4].bytes, 250);
    assert_eq!(segments[4].kind, MemSegmentKind::Other);
}

#[test]
fn swap_breakdown_is_absent_without_configured_swap() {
    assert!(swap_breakdown(&MemoryMetrics::default()).is_none());
    assert!(swap_breakdown(&memory(0, 0, Some((0, 0)), Default::default())).is_none());
}

#[test]
fn swap_breakdown_clamps_used_to_total() {
    let memory = memory(1, 0, Some((100, 250)), Default::default());
    let swap = swap_breakdown(&memory).expect("configured swap yields a breakdown");
    assert_eq!(swap.used_bytes, 100);
    assert_eq!(swap.total_bytes, 100);
    assert!(!swap.zswap_on);
}

#[test]
fn zfs_arc_renders_as_its_own_reclaimable_segment() {
    let memory = memory(
        1_000,
        400,
        None,
        MemoryCompositionObservations {
            buffers_bytes: OptionalObservation::present(50, 1),
            active_bytes: OptionalObservation::present(300, 1),
            inactive_bytes: OptionalObservation::present(200, 1),
            free_bytes: OptionalObservation::present(150, 1),
            reclaimable_bytes: OptionalObservation::present(50, 1),
            zfs_arc_bytes: OptionalObservation::present(120, 1),
            ..Default::default()
        },
    );
    let segments = memory_segments(&memory);
    assert_eq!(segments.len(), 6);
    // Without the ARC the 120 bytes landed in "other / reserved"; the
    // distinct segment must claim them and shrink Other by the same amount.
    let arc = segments
        .iter()
        .find(|segment| segment.kind == MemSegmentKind::ZfsArc)
        .expect("ARC renders as its own segment");
    assert_eq!(arc.bytes, 120);
    assert_eq!(
        segments.iter().map(|segment| segment.bytes).sum::<u64>(),
        1_000
    );
    let other = segments
        .iter()
        .find(|segment| segment.kind == MemSegmentKind::Other)
        .expect("other/reserved still renders");
    assert_eq!(other.bytes, 130);
}

#[test]
fn minimal_path_projects_arc_into_available_without_touching_kernel_facts() {
    // No page-state composition at all: the two-segment fallback. The
    // kernel available stays total − used; the bar's Available segment adds
    // the ARC on top so a ZFS host doesn't read as starved.
    let memory = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(1_000, 1),
            used_bytes: ScalarObservation::available(600, 1),
            available_bytes: ScalarObservation::available(400, 1),
            ..Default::default()
        },
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                zfs_arc_bytes: OptionalObservation::present(250, 1),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    assert_eq!(memory.current_available_bytes(), Some(400));
    let segments = memory_segments(&memory);
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[1].kind, MemSegmentKind::Available);
    assert_eq!(segments[1].bytes, 650);
}

#[test]
fn swap_breakdown_carries_the_zram_compression_depth() {
    let memory = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            swap_total_bytes: ScalarObservation::available(4_000, 1),
            swap_used_bytes: ScalarObservation::available(1_000, 1),
            ..Default::default()
        },
        MemoryOptionalObservations {
            compression: MemoryCompressionObservations {
                compressed_swap_original_bytes: OptionalObservation::present(3_000, 1),
                compressed_swap_compressed_bytes: OptionalObservation::present(1_000, 1),
                compressed_swap_memory_used_bytes: OptionalObservation::present(1_200, 1),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let swap = swap_breakdown(&memory).expect("configured swap yields a breakdown");
    assert_eq!(swap.zram_original_bytes, Some(3_000));
    assert_eq!(swap.zram_compressed_bytes, Some(1_000));
    assert_eq!(swap.zram_memory_used_bytes, Some(1_200));
    assert_eq!(swap.zram_compression_ratio, Some(3.0));

    // A zero or missing compressed size never fabricates a ratio.
    let empty = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            swap_total_bytes: ScalarObservation::available(4_000, 1),
            swap_used_bytes: ScalarObservation::available(1_000, 1),
            ..Default::default()
        },
        MemoryOptionalObservations {
            compression: MemoryCompressionObservations {
                compressed_swap_original_bytes: OptionalObservation::present(3_000, 1),
                compressed_swap_compressed_bytes: OptionalObservation::present(0, 1),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(
        swap_breakdown(&empty)
            .expect("swap renders")
            .zram_compression_ratio,
        None
    );
}
