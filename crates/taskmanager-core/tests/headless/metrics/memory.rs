use super::*;
use crate::core::metrics::OptionalObservation;

fn scalar_truth(total: u64, used: u64, at_ms: u64) -> MemoryScalarObservations {
    MemoryScalarObservations {
        total_bytes: ScalarObservation::available(total, at_ms),
        used_bytes: ScalarObservation::available(used, at_ms),
        available_bytes: ScalarObservation::available(total.saturating_sub(used), at_ms),
        swap_total_bytes: ScalarObservation::available(0, at_ms),
        swap_used_bytes: ScalarObservation::available(0, at_ms),
        used_rate_mib_per_sec: ScalarObservation::available(0.0, at_ms),
    }
}

#[test]
fn typed_memory_scalars_distinguish_real_zero_from_unavailable() {
    let observed = MemoryMetrics::from_observations(scalar_truth(0, 0, 10), Default::default());
    assert_eq!(observed.current_total_bytes(), Some(0));
    assert_eq!(observed.current_used_bytes(), Some(0));
    assert_eq!(observed.current_used_rate_mib_per_sec(), Some(0.0));

    let unavailable = MemoryMetrics::from_observations(
        MemoryScalarObservations::unavailable(FailureKind::PermissionDenied),
        MemoryOptionalObservations::unavailable(FailureKind::PermissionDenied),
    );
    assert_eq!(unavailable.current_total_bytes(), None);
    assert_eq!(unavailable.current_used_bytes(), None);
    assert_eq!(unavailable.current_used_rate_mib_per_sec(), None);
}

#[test]
fn legacy_only_wire_hydrates_when_total_is_a_trustworthy_denominator() {
    let memory: MemoryMetrics = serde_json::from_value(serde_json::json!({
        "total_bytes": 100,
        "used_bytes": 40,
        "available_bytes": 60,
        "swap_total_bytes": 10,
        "swap_used_bytes": 2,
        "cached_bytes": 0,
        "mem_used_rate_mbps": 1.5
    }))
    .expect("legacy memory payload should hydrate canonical observations");

    assert_eq!(memory.current_total_bytes(), Some(100));
    assert_eq!(memory.used_percentage_observed(), Some(40.0));
    assert_eq!(memory.current_cached_bytes(), Some(0));
    assert_eq!(memory.current_used_rate_mib_per_sec(), Some(1.5));
}

#[test]
fn typed_only_wire_round_trips_without_legacy_keys() {
    let scalar = scalar_truth(100, 25, 10);
    let optional = MemoryOptionalObservations {
        modules: MemoryModuleObservations {
            speed_mhz: OptionalObservation::present(8_533, 10),
            module_type: OptionalObservation::present("LPDDR5".into(), 10),
            ..Default::default()
        },
        ..Default::default()
    };
    let memory: MemoryMetrics = serde_json::from_value(serde_json::json!({
        "scalar_observations": scalar,
        "optional_observations": optional
    }))
    .expect("typed-only memory payload should deserialize");

    assert_eq!(memory.current_total_bytes(), Some(100));
    assert_eq!(memory.current_used_bytes(), Some(25));
    assert_eq!(memory.current_speed_mhz(), Some(8_533));
    assert_eq!(memory.current_module_type(), Some("LPDDR5"));
}

#[test]
fn typed_failure_wins_over_conflicting_legacy_success() {
    let memory: MemoryMetrics = serde_json::from_value(serde_json::json!({
        "total_bytes": 100,
        "used_bytes": 40,
        "cached_bytes": 99,
        "scalar_observations": MemoryScalarObservations::unavailable(
            FailureKind::TemporarilyUnavailable
        ),
        "optional_observations": MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                cached_bytes: OptionalObservation::unavailable(FailureKind::TimedOut),
                ..Default::default()
            },
            ..Default::default()
        }
    }))
    .expect("mixed memory payload should deserialize");

    assert_eq!(memory.current_total_bytes(), None);
    assert_eq!(memory.current_used_bytes(), None);
    assert_eq!(memory.current_cached_bytes(), None);
    assert_eq!(
        memory
            .optional_observations()
            .composition
            .cached_bytes
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::TimedOut)
    );
}

#[test]
fn failure_and_confirmed_absence_never_serialize_as_legacy_success() {
    let metrics = MemoryMetrics::from_observations(
        MemoryScalarObservations::unavailable(FailureKind::PermissionDenied),
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                cached_bytes: OptionalObservation::absent(10),
                buffers_bytes: OptionalObservation::unavailable(FailureKind::Unsupported),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let wire = serde_json::to_value(metrics).expect("memory metrics should serialize");
    assert!(wire.get("total_bytes").is_none());
    assert!(wire.get("cached_bytes").is_none());
    assert!(wire.get("buffers_bytes").is_none());
}

#[test]
fn legacy_optional_values_require_a_trustworthy_memory_identity() {
    let missing_identity: MemoryMetrics =
        serde_json::from_value(serde_json::json!({ "cached_bytes": 0 }))
            .expect("legacy memory payload should deserialize");
    assert_eq!(missing_identity.current_cached_bytes(), None);

    let with_identity: MemoryMetrics = serde_json::from_value(serde_json::json!({
        "total_bytes": 1_024,
        "cached_bytes": 0
    }))
    .expect("legacy memory payload should deserialize");
    assert_eq!(with_identity.current_cached_bytes(), Some(0));
}

#[test]
fn failed_refresh_retains_scalar_and_optional_truth_only_as_stale() {
    let previous = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            used_rate_mib_per_sec: ScalarObservation::available(3.5, 10),
            ..Default::default()
        },
        MemoryOptionalObservations {
            compression: MemoryCompressionObservations {
                compressed_swap_cache_enabled: OptionalObservation::absent(10),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut current = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            used_rate_mib_per_sec: ScalarObservation::unavailable(
                FailureKind::TemporarilyUnavailable,
            ),
            ..Default::default()
        },
        MemoryOptionalObservations {
            compression: MemoryCompressionObservations {
                compressed_swap_cache_enabled: OptionalObservation::unavailable(
                    FailureKind::PermissionDenied,
                ),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    current.retain_previous_observations(&previous);

    let rate = current.scalar_observations().used_rate_mib_per_sec;
    assert_eq!(
        rate.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(rate.current_value(), None);
    assert_eq!(rate.last_known_value(), Some(&3.5));
    let compressed_swap = &current
        .optional_observations()
        .compression
        .compressed_swap_cache_enabled;
    assert_eq!(
        compressed_swap.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(compressed_swap.current_value(), None);
    assert_eq!(compressed_swap.last_success_ms(), Some(10));
}

#[test]
fn zfs_arc_layers_onto_reclaimable_and_availability_without_redefining_facts() {
    fn memory_with(arc: OptionalObservation<u64>) -> MemoryMetrics {
        MemoryMetrics::from_observations(
            scalar_truth(1_000, 600, 10),
            MemoryOptionalObservations {
                composition: MemoryCompositionObservations {
                    reclaimable_bytes: OptionalObservation::present(100, 10),
                    zfs_arc_bytes: arc,
                    ..Default::default()
                },
                ..Default::default()
            },
        )
    }

    // Without an ARC the projections equal the raw kernel facts.
    let without_arc = memory_with(OptionalObservation::absent(10));
    assert_eq!(without_arc.current_reclaimable_with_arc_bytes(), Some(100));
    assert_eq!(without_arc.projected_available_bytes(), Some(400));

    // With an ARC the fold sums the reclaimable family and availability
    // gains the cache — while the kernel facts stay exactly as reported.
    let with_arc = memory_with(OptionalObservation::present(300, 10));
    assert_eq!(with_arc.current_reclaimable_with_arc_bytes(), Some(400));
    assert_eq!(with_arc.projected_available_bytes(), Some(700));
    assert_eq!(with_arc.current_available_bytes(), Some(400));
    assert_eq!(with_arc.current_reclaimable_bytes(), Some(100));
    assert_eq!(with_arc.current_zfs_arc_bytes(), Some(300));

    // An unavailable kernel availability is never papered over by the ARC.
    let no_kernel_available = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            available_bytes: ScalarObservation::unavailable(FailureKind::TimedOut),
            ..scalar_truth(1_000, 600, 10)
        },
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                zfs_arc_bytes: OptionalObservation::present(300, 10),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    assert_eq!(no_kernel_available.projected_available_bytes(), None);
}

#[test]
fn compressed_swap_ratio_is_guarded_against_zero_and_missing_inputs() {
    fn compression(
        original: OptionalObservation<u64>,
        compressed: OptionalObservation<u64>,
    ) -> MemoryCompressionObservations {
        MemoryCompressionObservations {
            compressed_swap_original_bytes: original,
            compressed_swap_compressed_bytes: compressed,
            ..Default::default()
        }
    }

    let healthy = compression(
        OptionalObservation::present(3_221_225_472, 10),
        OptionalObservation::present(1_073_741_824, 10),
    );
    assert_eq!(healthy.compression_ratio(), Some(3.0));

    // A zero or unavailable denominator must not fabricate 0:1 or infinity.
    assert_eq!(
        compression(
            OptionalObservation::present(3_221_225_472, 10),
            OptionalObservation::present(0, 10),
        )
        .compression_ratio(),
        None
    );
    assert_eq!(
        compression(
            OptionalObservation::present(3_221_225_472, 10),
            OptionalObservation::unavailable(FailureKind::ProviderFault),
        )
        .compression_ratio(),
        None
    );
    // The numerator is equally required.
    assert_eq!(
        compression(
            OptionalObservation::absent(10),
            OptionalObservation::present(1_073_741_824, 10),
        )
        .compression_ratio(),
        None
    );

    // The MemoryMetrics accessor delegates to the same pure rule.
    let memory = MemoryMetrics::from_observations(
        MemoryScalarObservations::default(),
        MemoryOptionalObservations {
            compression: healthy,
            ..Default::default()
        },
    );
    assert_eq!(memory.current_compressed_swap_ratio(), Some(3.0));
}

#[test]
fn older_memory_payloads_default_the_zfs_and_mm_stat_facts_to_unknown() {
    // A payload written before these fields existed (no `zfs_arc_bytes`,
    // no mm_stat compression keys) must decode with the new facts unknown,
    // never fail, and never hydrate a fabricated zero.
    let memory = MemoryMetrics::from_observations(
        scalar_truth(1_000, 400, 10),
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                cached_bytes: OptionalObservation::present(100, 10),
                reclaimable_bytes: OptionalObservation::present(50, 10),
                zfs_arc_bytes: OptionalObservation::present(250, 10),
                ..Default::default()
            },
            compression: MemoryCompressionObservations {
                compressed_swap_used_bytes: OptionalObservation::present(700, 10),
                compressed_swap_original_bytes: OptionalObservation::present(2_100, 10),
                compressed_swap_compressed_bytes: OptionalObservation::present(700, 10),
                compressed_swap_memory_used_bytes: OptionalObservation::present(800, 10),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let mut value = serde_json::to_value(&memory).expect("memory metrics serialize");
    let strip = |group: &mut serde_json::Value, keys: &[&str]| {
        for key in keys {
            group
                .as_object_mut()
                .expect("observation group serializes as an object")
                .remove(*key);
        }
    };
    strip(
        value
            .pointer_mut("/optional_observations/composition")
            .expect("composition group serializes"),
        &["zfs_arc_bytes"],
    );
    strip(
        value
            .pointer_mut("/optional_observations/compression")
            .expect("compression group serializes"),
        &[
            "compressed_swap_original_bytes",
            "compressed_swap_compressed_bytes",
            "compressed_swap_memory_used_bytes",
        ],
    );

    let decoded: MemoryMetrics =
        serde_json::from_value(value).expect("pre-ARC payload should deserialize");

    assert_eq!(decoded.current_zfs_arc_bytes(), None);
    assert_eq!(decoded.current_compressed_swap_original_bytes(), None);
    assert_eq!(decoded.current_compressed_swap_compressed_bytes(), None);
    assert_eq!(decoded.current_compressed_swap_ratio(), None);
    assert_eq!(
        decoded
            .optional_observations()
            .composition
            .zfs_arc_bytes
            .availability(),
        ScalarAvailability::Unknown
    );
    // Sibling facts from the same payload stay intact.
    assert_eq!(decoded.current_cached_bytes(), Some(100));
    assert_eq!(decoded.current_compressed_swap_used_bytes(), Some(700));
}
