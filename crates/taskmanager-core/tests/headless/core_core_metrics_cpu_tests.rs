use super::*;

#[test]
fn measured_zero_is_current_but_default_zero_is_unknown() {
    let default_metrics = CpuMetrics::default();
    assert_eq!(default_metrics.current_global_usage_pct(), None);

    let mut observed = CpuMetrics::default();
    observed.apply_scalar_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(0.0, 10),
        frequency_mhz: ScalarObservation::available(0, 10),
        temperature_c: ScalarObservation::available(0.0, 10),
        power_w: ScalarObservation::available(0.0, 10),
        ..Default::default()
    });
    assert_eq!(observed.current_global_usage_pct(), Some(0.0));
    assert_eq!(observed.current_frequency_mhz(), Some(0));
    assert_eq!(observed.current_temperature_c(), Some(0.0));
    assert_eq!(observed.current_power_w(), Some(0.0));
}

#[test]
fn explicit_failure_never_falls_back_to_legacy_projection() {
    let metrics: CpuMetrics = serde_json::from_value(serde_json::json!({
        "brand": "fixture CPU",
        "global_usage": 75.0,
        "core_usages": [75.0],
        "frequency_mhz": 3200,
        "scalar_observations": {
            "global_usage_pct": ScalarObservation::<f32>::unavailable(FailureKind::PermissionDenied),
            "frequency_mhz": ScalarObservation::<u64>::unavailable(FailureKind::ProviderFault)
        }
    }))
    .expect("mixed CPU wire");

    assert_eq!(metrics.current_global_usage_pct(), None);
    assert_eq!(metrics.current_frequency_mhz(), None);
}

#[test]
fn typed_failure_omits_legacy_success_keys_but_confirmed_empty_is_preserved() {
    let failed = CpuMetrics::from_observations(CpuScalarObservations::unavailable(
        FailureKind::PermissionDenied,
    ));
    let failed_wire = serde_json::to_value(failed).expect("failed CPU metrics should serialize");
    for legacy_key in [
        "global_usage",
        "core_usages",
        "frequency_mhz",
        "max_freq_mhz",
        "per_core_freq_mhz",
        "temperature_c",
        "per_core_temps_c",
        "cpu_power_w",
    ] {
        assert!(
            failed_wire.get(legacy_key).is_none(),
            "{legacy_key} must not turn a typed failure into legacy success"
        );
    }

    let confirmed_empty = CpuMetrics::from_observations(CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::available(Vec::new(), 10),
        ..Default::default()
    });
    let confirmed_empty_wire =
        serde_json::to_value(confirmed_empty).expect("confirmed-empty CPU group should serialize");
    assert_eq!(confirmed_empty_wire["core_usages"], serde_json::json!([]));
    assert_eq!(
        confirmed_empty_wire["scalar_observations"]["core_usage_pct"],
        serde_json::json!([])
    );
}

#[test]
fn prior_success_is_retained_only_as_stale() {
    let previous = CpuScalarObservations {
        power_w: ScalarObservation::available(12.5, 10),
        ..Default::default()
    };
    let current = CpuScalarObservations {
        power_w: ScalarObservation::unavailable(FailureKind::TemporarilyUnavailable),
        ..Default::default()
    }
    .retain_previous(previous);

    assert_eq!(current.power_w.current_value(), None);
    assert_eq!(current.power_w.last_known_value(), Some(&12.5));
    assert_eq!(
        current.power_w.availability(),
        ScalarAvailability::Stale(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(current.power_w.last_success_ms(), Some(10));
}

#[test]
fn known_group_truth_never_falls_back_to_legacy_vectors() {
    let metrics = CpuMetrics::from_observations(CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::unavailable(FailureKind::TemporarilyUnavailable),
        per_core_frequency_group: ScalarObservationGroup::unavailable(FailureKind::Unsupported),
        per_core_temperature_group: ScalarObservationGroup::available(Vec::new(), 10),
        ..Default::default()
    });

    assert_eq!(metrics.current_core_usage_pct(0), None);
    assert_eq!(metrics.current_core_usage_len(), 0);
    assert_eq!(metrics.current_core_frequency_mhz(0), None);
    assert_eq!(metrics.current_core_temperature_c(0), None);
    assert_eq!(metrics.current_core_temperature_len(), 0);
    assert_eq!(
        metrics
            .scalar_observations()
            .per_core_temperature_group
            .current_observations(),
        Some(&[][..])
    );
}

#[test]
fn per_core_group_failure_retains_slots_only_as_stale() {
    let previous = CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::available(vec![25.0], 10),
        ..Default::default()
    };
    let current = CpuScalarObservations {
        core_usage_group: ScalarObservationGroup::unavailable(FailureKind::PermissionDenied),
        ..Default::default()
    }
    .retain_previous(previous);

    assert_eq!(
        current.core_usage_group.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(current.core_usage_group.last_success_ms(), Some(10));
    assert_eq!(current.core_usage_group.current_observations(), None);
    assert_eq!(
        current.core_usage_group.last_known_observations()[0].availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        current.core_usage_group.last_known_observations()[0].last_known_value(),
        Some(&25.0)
    );
}

#[test]
fn frequency_source_is_explicit_only_for_bogomips_fallback() {
    let native = serde_json::to_value(CpuMetrics::default()).expect("native CPU JSON");
    assert!(native.get("frequency_source").is_none());

    let mut fallback = CpuMetrics::from_observations(CpuScalarObservations {
        frequency_mhz: ScalarObservation::available(2_400, 10),
        ..Default::default()
    });
    fallback.frequency_source = CpuFrequencySource::BogoMips;
    let encoded = serde_json::to_value(&fallback).expect("fallback CPU JSON");
    assert_eq!(encoded["frequency_source"], "bogo_mips");

    let decoded: CpuMetrics =
        serde_json::from_value(encoded).expect("fallback CPU JSON should decode");
    assert_eq!(decoded.frequency_source, CpuFrequencySource::BogoMips);
    assert!(decoded.frequency_source.is_bogomips());
}

#[test]
fn temperature_source_round_trips_every_non_default_tier() {
    // The default chip tier keeps pre-provenance payloads byte-compatible.
    let default_chip = serde_json::to_value(CpuMetrics::default()).expect("default CPU JSON");
    assert!(default_chip.get("temperature_source").is_none());

    let with_source = |source| {
        let mut metrics = CpuMetrics::from_observations(CpuScalarObservations {
            temperature_c: ScalarObservation::available(51.0, 10),
            ..Default::default()
        });
        metrics.temperature_source = source;
        metrics
    };
    // The other native chips serialize explicitly: which driver produced the
    // reading is diagnostic truth worth preserving (Steam-Deck-class bugs).
    for (metrics, wire) in [
        (with_source(CpuTemperatureSource::K10temp), "k10temp"),
        (with_source(CpuTemperatureSource::Zenpower), "zenpower"),
        (
            with_source(CpuTemperatureSource::PackageHwmon),
            "package_hwmon",
        ),
        (
            with_source(CpuTemperatureSource::ThermalZone),
            "thermal_zone",
        ),
    ] {
        let encoded = serde_json::to_value(&metrics).expect("CPU JSON");
        assert_eq!(encoded["temperature_source"], wire);
        let decoded: CpuMetrics = serde_json::from_value(encoded).expect("CPU JSON decodes");
        assert_eq!(decoded.temperature_source, metrics.temperature_source);
        assert_eq!(
            decoded.temperature_source.is_labeled_fallback(),
            metrics.temperature_source.is_labeled_fallback()
        );
    }
}

#[test]
fn usage_gate_rejects_phantom_percentages_and_keeps_measured_bounds() {
    let gap = ScalarObservation::<f32>::unavailable(FailureKind::ProviderFault);
    assert_eq!(cpu_usage_pct_observation(f32::NAN, 10), gap);
    assert_eq!(cpu_usage_pct_observation(f32::INFINITY, 10), gap);
    assert_eq!(cpu_usage_pct_observation(f32::NEG_INFINITY, 10), gap);
    assert_eq!(cpu_usage_pct_observation(-1.0, 10), gap);
    assert_eq!(cpu_usage_pct_observation(100.5, 10), gap);
    assert_eq!(cpu_usage_pct_observation(3.4e38, 10), gap);

    // A measured zero and a fully busy core are real values, not gaps.
    assert_eq!(
        cpu_usage_pct_observation(0.0, 10),
        ScalarObservation::available(0.0, 10)
    );
    assert_eq!(
        cpu_usage_pct_observation(100.0, 20),
        ScalarObservation::available(100.0, 20)
    );
    // Saturation rounding inside the tolerance clamps instead of spiking.
    assert_eq!(
        cpu_usage_pct_observation(100.2, 30),
        ScalarObservation::available(100.0, 30)
    );
}
