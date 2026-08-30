use super::{
    CpuMetrics, CpuPerformancePolicy, GpuMetrics, GpuScalarObservations, GpuThrottleReason,
    MemoryCompositionObservations, MemoryCompressionObservations, MemoryMetrics,
    MemoryOptionalObservations, MemoryScalarObservations, NetworkAdapterType, NetworkMetrics,
    NetworkScalarObservations, NetworkWirelessObservations, OptionalObservation,
    OptionalObservationState, ScalarAvailability, ScalarObservation,
    VirtualMemoryCommitObservations,
};
use crate::core::FailureKind;

const OPTIONAL_CPU_OBSERVATIONS: [&str; 4] = [
    "frequency_mhz",
    "max_freq_mhz",
    "per_core_freq_mhz",
    "temperature_c",
];

#[test]
fn missing_cpu_observations_deserialize_as_unknown_instead_of_zero() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("CPU metrics should serialize as an object");
    for field in OPTIONAL_CPU_OBSERVATIONS {
        fields.remove(field);
    }

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("missing optional observations should deserialize");

    assert_eq!(decoded.current_frequency_mhz(), None);
    assert_eq!(decoded.current_max_frequency_mhz(), None);
    assert_eq!(decoded.current_core_frequency_len(), 0);
    assert_eq!(decoded.current_temperature_c(), None);
}

#[test]
fn legacy_numeric_cpu_observations_deserialize_as_present_values() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("CPU metrics should serialize as an object");
    fields.insert("brand".into(), serde_json::json!("Example CPU"));
    fields.insert("frequency_mhz".into(), serde_json::json!(3_200));
    fields.insert("max_freq_mhz".into(), serde_json::json!(4_400));
    fields.insert(
        "per_core_freq_mhz".into(),
        serde_json::json!([3_200, 3_100, 0]),
    );
    fields.insert("temperature_c".into(), serde_json::json!(0.0));

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("legacy numeric observations should deserialize");

    assert_eq!(decoded.current_frequency_mhz(), Some(3_200));
    assert_eq!(decoded.current_max_frequency_mhz(), Some(4_400));
    assert_eq!(
        (0..decoded.current_core_frequency_len())
            .map(|index| decoded.current_core_frequency_mhz(index))
            .collect::<Vec<_>>(),
        [Some(3_200), Some(3_100), Some(0)]
    );
    assert_eq!(decoded.current_temperature_c(), Some(0.0));
}

#[test]
fn pre_group_cpu_scalar_json_keeps_legacy_per_core_observations() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let scalar_fields = value["scalar_observations"]
        .as_object_mut()
        .expect("CPU scalar observations should serialize as an object");
    for group in [
        "core_usage_group",
        "per_core_frequency_group",
        "per_core_temperature_group",
    ] {
        scalar_fields.remove(group);
    }
    scalar_fields.insert(
        "core_usage_pct".into(),
        serde_json::json!([ScalarObservation::available(0.0_f32, 10)]),
    );
    scalar_fields.insert(
        "per_core_frequency_mhz".into(),
        serde_json::json!([ScalarObservation::available(0_u64, 10)]),
    );
    scalar_fields.insert(
        "per_core_temperature_c".into(),
        serde_json::json!([ScalarObservation::available(0.0_f32, 10)]),
    );

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("pre-group CPU scalar JSON should deserialize");

    assert_eq!(
        decoded
            .scalar_observations()
            .core_usage_group
            .availability(),
        ScalarAvailability::Available
    );
    assert_eq!(
        decoded
            .scalar_observations()
            .per_core_frequency_group
            .availability(),
        ScalarAvailability::Available
    );
    assert_eq!(
        decoded
            .scalar_observations()
            .per_core_temperature_group
            .availability(),
        ScalarAvailability::Available
    );
    assert_eq!(decoded.current_core_usage_pct(0), Some(0.0));
    assert_eq!(decoded.current_core_frequency_mhz(0), Some(0));
    assert_eq!(decoded.current_core_temperature_c(0), Some(0.0));
}

#[test]
fn legacy_cpu_zero_requires_supporting_payload_to_be_a_current_measurement() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("CPU metrics should serialize as an object");
    fields.remove("scalar_observations");
    fields.insert("global_usage".into(), serde_json::json!(0.0));
    fields.insert("core_usages".into(), serde_json::json!([0.0]));
    fields.insert("frequency_mhz".into(), serde_json::json!(0));
    fields.insert("temperature_c".into(), serde_json::json!(0.0));
    fields.insert("cpu_power_w".into(), serde_json::json!(0.0));

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("legacy CPU metrics should deserialize");

    assert_eq!(decoded.current_global_usage_pct(), Some(0.0));
    assert_eq!(decoded.current_core_usage_pct(0), Some(0.0));
    assert_eq!(decoded.current_frequency_mhz(), Some(0));
    assert_eq!(decoded.current_temperature_c(), Some(0.0));
    assert_eq!(decoded.current_power_w(), Some(0.0));

    let default_legacy: CpuMetrics = serde_json::from_value({
        let mut value =
            serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
        value
            .as_object_mut()
            .expect("CPU metrics should serialize as an object")
            .remove("scalar_observations");
        value
    })
    .expect("legacy default CPU metrics should deserialize");
    assert_eq!(default_legacy.current_global_usage_pct(), None);
}

#[test]
fn legacy_cpu_identity_and_topology_scalars_deserialize_as_observed_values() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("CPU metrics should serialize as an object");
    fields.insert("brand".into(), serde_json::json!("Example CPU"));
    fields.insert("physical_cores".into(), serde_json::json!(8));
    fields.insert("logical_cores".into(), serde_json::json!(16));
    fields.insert("l1d_cache_kb".into(), serde_json::json!(512));
    fields.insert("l1i_cache_kb".into(), serde_json::json!(256));
    fields.insert("l2_cache_kb".into(), serde_json::json!(8_192));
    fields.insert("l3_cache_kb".into(), serde_json::json!(32_768));

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("legacy topology scalars should deserialize");

    assert_eq!(decoded.brand.as_deref(), Some("Example CPU"));
    assert_eq!(decoded.physical_cores, Some(8));
    assert_eq!(decoded.logical_cores, Some(16));
    assert_eq!(decoded.l1d_cache_kb, Some(512));
    assert_eq!(decoded.l1i_cache_kb, Some(256));
    assert_eq!(decoded.l2_cache_kb, Some(8_192));
    assert_eq!(decoded.l3_cache_kb, Some(32_768));
}

#[test]
fn legacy_top_level_cpu_policy_json_deserializes_into_neutral_model() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("CPU metrics should serialize as an object");
    fields.insert("cpufreq_driver".into(), serde_json::json!("intel_pstate"));
    fields.insert("cpufreq_governor".into(), serde_json::json!("powersave"));
    fields.insert(
        "power_preference".into(),
        serde_json::json!("balance_performance"),
    );

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("legacy CPU policy JSON should deserialize");

    assert_eq!(
        decoded.performance_policy,
        CpuPerformancePolicy {
            frequency_implementation: Some("intel_pstate".into()),
            active_policy: Some("powersave".into()),
            energy_preference: Some("balance_performance".into()),
        }
    );
}

#[test]
fn neutral_cpu_policy_serializes_with_legacy_top_level_json_keys() {
    let mut metrics = CpuMetrics::default();
    metrics.performance_policy = CpuPerformancePolicy {
        frequency_implementation: Some("native-frequency-control".into()),
        active_policy: Some("balanced".into()),
        energy_preference: Some("efficiency".into()),
    };

    let value = serde_json::to_value(metrics).expect("CPU metrics should serialize");

    assert_eq!(value["cpufreq_driver"], "native-frequency-control");
    assert_eq!(value["cpufreq_governor"], "balanced");
    assert_eq!(value["power_preference"], "efficiency");
    assert!(value.get("performance_policy").is_none());
    assert!(value.get("frequency_implementation").is_none());
    assert!(value.get("active_policy").is_none());
    assert!(value.get("energy_preference").is_none());
}

#[test]
fn neutral_cpu_policy_json_aliases_are_accepted() {
    let mut value =
        serde_json::to_value(CpuMetrics::default()).expect("CPU metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("CPU metrics should serialize as an object");
    fields.remove("cpufreq_driver");
    fields.remove("cpufreq_governor");
    fields.remove("power_preference");
    fields.insert(
        "frequency_implementation".into(),
        serde_json::json!("native-policy-api"),
    );
    fields.insert("active_policy".into(), serde_json::json!("automatic"));
    fields.insert(
        "energy_preference".into(),
        serde_json::json!("battery_saver"),
    );

    let decoded: CpuMetrics =
        serde_json::from_value(value).expect("neutral CPU policy aliases should deserialize");

    assert_eq!(
        decoded.performance_policy,
        CpuPerformancePolicy {
            frequency_implementation: Some("native-policy-api".into()),
            active_policy: Some("automatic".into()),
            energy_preference: Some("battery_saver".into()),
        }
    );
}

#[test]
fn second_platform_can_construct_cpu_policy_without_linux_vocabulary() {
    let policy = CpuPerformancePolicy {
        frequency_implementation: Some("processor-power-management".into()),
        active_policy: Some("balanced".into()),
        energy_preference: Some("best-power-efficiency".into()),
    };
    let mut metrics = CpuMetrics::default();
    metrics.performance_policy = policy.clone();

    assert_eq!(metrics.performance_policy, policy);
}

#[test]
fn legacy_top_level_memory_json_deserializes_into_optional_groups() {
    let mut value =
        serde_json::to_value(MemoryMetrics::default()).expect("memory metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("memory metrics should serialize as an object");
    fields.insert("total_bytes".into(), serde_json::json!(1_024));
    for (key, value) in [
        ("active_bytes", serde_json::json!(100)),
        ("inactive_bytes", serde_json::json!(200)),
        ("mem_free_bytes", serde_json::json!(300)),
        ("slab_reclaimable_bytes", serde_json::json!(400)),
        ("committed_bytes", serde_json::json!(500)),
        ("commit_limit_bytes", serde_json::json!(600)),
        ("zram_swap_used_bytes", serde_json::json!(700)),
        ("zram_total_bytes", serde_json::json!(800)),
        ("zswap_enabled", serde_json::json!(true)),
    ] {
        fields.insert(key.into(), value);
    }

    let decoded: MemoryMetrics =
        serde_json::from_value(value).expect("legacy memory JSON should deserialize");

    assert_eq!(decoded.current_active_bytes(), Some(100));
    assert_eq!(decoded.current_inactive_bytes(), Some(200));
    assert_eq!(decoded.current_free_bytes(), Some(300));
    assert_eq!(decoded.current_reclaimable_bytes(), Some(400));
    assert_eq!(decoded.current_committed_bytes(), Some(500));
    assert_eq!(decoded.current_commit_limit_bytes(), Some(600));
    assert_eq!(decoded.current_compressed_memory_used_bytes(), None);
    assert_eq!(decoded.current_compressed_swap_used_bytes(), Some(700));
    assert_eq!(decoded.current_compressed_swap_capacity_bytes(), Some(800));
    assert_eq!(decoded.current_compressed_swap_cache_enabled(), Some(true));
}

#[test]
fn legacy_memory_availability_scalars_deserialize_as_observed_values() {
    let mut value =
        serde_json::to_value(MemoryMetrics::default()).expect("memory metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("memory metrics should serialize as an object");
    fields.insert("total_bytes".into(), serde_json::json!(1_024));
    fields.insert("cached_bytes".into(), serde_json::json!(100));
    fields.insert("buffers_bytes".into(), serde_json::json!(200));
    fields.insert("hardware_reserved_bytes".into(), serde_json::json!(300));
    fields.insert("mem_used_rate_mbps".into(), serde_json::json!(0.0));

    let decoded: MemoryMetrics =
        serde_json::from_value(value).expect("legacy memory scalars should deserialize");

    assert_eq!(decoded.current_cached_bytes(), Some(100));
    assert_eq!(decoded.current_buffers_bytes(), Some(200));
    assert_eq!(decoded.current_hardware_reserved_bytes(), Some(300));
    assert_eq!(decoded.current_used_rate_mib_per_sec(), Some(0.0));
}

#[test]
fn neutral_memory_groups_serialize_with_legacy_top_level_json_keys() {
    let metrics = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(1_024, 10),
            ..Default::default()
        },
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                active_bytes: OptionalObservation::present(100, 10),
                inactive_bytes: OptionalObservation::present(200, 10),
                free_bytes: OptionalObservation::present(300, 10),
                reclaimable_bytes: OptionalObservation::present(400, 10),
                ..Default::default()
            },
            virtual_memory_commit: VirtualMemoryCommitObservations {
                committed_bytes: OptionalObservation::present(500, 10),
                limit_bytes: OptionalObservation::present(600, 10),
            },
            compression: MemoryCompressionObservations {
                compressed_swap_used_bytes: OptionalObservation::present(700, 10),
                compressed_swap_capacity_bytes: OptionalObservation::present(800, 10),
                compressed_swap_cache_enabled: OptionalObservation::present(false, 10),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    let value = serde_json::to_value(metrics).expect("memory metrics should serialize");

    for (key, expected) in [
        ("active_bytes", serde_json::json!(100)),
        ("inactive_bytes", serde_json::json!(200)),
        ("mem_free_bytes", serde_json::json!(300)),
        ("slab_reclaimable_bytes", serde_json::json!(400)),
        ("committed_bytes", serde_json::json!(500)),
        ("commit_limit_bytes", serde_json::json!(600)),
        ("zram_swap_used_bytes", serde_json::json!(700)),
        ("zram_total_bytes", serde_json::json!(800)),
        ("zswap_enabled", serde_json::json!(false)),
    ] {
        assert_eq!(value[key], expected);
    }
    for nested_or_alias in [
        "composition",
        "virtual_memory_commit",
        "compression",
        "free_bytes",
        "reclaimable_bytes",
        "limit_bytes",
        "compressed_swap_used_bytes",
        "compressed_swap_capacity_bytes",
        "compressed_swap_cache_enabled",
    ] {
        assert!(value.get(nested_or_alias).is_none());
    }
}

#[test]
fn neutral_memory_json_aliases_deserialize_without_provider_vocabulary() {
    let mut value =
        serde_json::to_value(MemoryMetrics::default()).expect("memory metrics should serialize");
    let fields = value
        .as_object_mut()
        .expect("memory metrics should serialize as an object");
    fields.insert("total_bytes".into(), serde_json::json!(1_024));
    for legacy in [
        "mem_free_bytes",
        "slab_reclaimable_bytes",
        "commit_limit_bytes",
        "zram_swap_used_bytes",
        "zram_total_bytes",
        "zswap_enabled",
    ] {
        fields.remove(legacy);
    }
    for (key, value) in [
        ("free_bytes", serde_json::json!(300)),
        ("reclaimable_bytes", serde_json::json!(400)),
        ("limit_bytes", serde_json::json!(600)),
        ("compressed_swap_used_bytes", serde_json::json!(700)),
        ("compressed_swap_capacity_bytes", serde_json::json!(800)),
        ("compressed_swap_cache_enabled", serde_json::json!(true)),
    ] {
        fields.insert(key.into(), value);
    }

    let decoded: MemoryMetrics =
        serde_json::from_value(value).expect("neutral memory aliases should deserialize");

    assert_eq!(decoded.current_free_bytes(), Some(300));
    assert_eq!(decoded.current_reclaimable_bytes(), Some(400));
    assert_eq!(decoded.current_commit_limit_bytes(), Some(600));
    assert_eq!(decoded.current_compressed_swap_used_bytes(), Some(700));
    assert_eq!(decoded.current_compressed_swap_capacity_bytes(), Some(800));
    assert_eq!(decoded.current_compressed_swap_cache_enabled(), Some(true));
}

#[test]
fn second_platform_can_report_compressed_memory_without_faking_compressed_swap() {
    let metrics = MemoryMetrics::from_observations(
        MemoryScalarObservations::default(),
        MemoryOptionalObservations {
            virtual_memory_commit: VirtualMemoryCommitObservations {
                committed_bytes: OptionalObservation::present(12_000, 10),
                limit_bytes: OptionalObservation::present(24_000, 10),
            },
            compression: MemoryCompressionObservations {
                compressed_memory_used_bytes: OptionalObservation::present(2_000, 10),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    assert_eq!(metrics.current_compressed_memory_used_bytes(), Some(2_000));
    assert_eq!(metrics.current_compressed_swap_used_bytes(), None);
    assert_eq!(metrics.current_compressed_swap_capacity_bytes(), None);
    assert_eq!(metrics.current_compressed_swap_cache_enabled(), None);
}

#[test]
fn gpu_throttle_reasons_are_provider_neutral_and_round_trip() {
    let reasons = vec![
        GpuThrottleReason::Idle,
        GpuThrottleReason::ApplicationClockLimit,
        GpuThrottleReason::SoftwarePowerLimit,
        GpuThrottleReason::HardwareSlowdown,
        GpuThrottleReason::ReliabilityLimit,
        GpuThrottleReason::SyncBoost,
        GpuThrottleReason::SoftwareThermalLimit,
        GpuThrottleReason::HardwareThermalLimit,
        GpuThrottleReason::ExternalPowerBrake,
        GpuThrottleReason::DisplayClockLimit,
        GpuThrottleReason::Other,
    ];
    let mut metrics = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(0.0, 10),
        idle_residency_pct: ScalarObservation::available(100.0, 10),
        ..Default::default()
    });
    metrics.apply_throttle_observation(ScalarObservation::available(reasons.clone(), 10));

    let json = serde_json::to_string(&metrics).expect("GPU metrics should serialize");
    assert!(json.contains("\"software_power_limit\""));
    assert!(json.contains("\"reliability_limit\""));
    assert!(json.contains("\"hardware_thermal_limit\""));
    assert!(json.contains("\"external_power_brake\""));
    assert!(json.contains("\"display_clock_limit\""));
    assert!(json.contains("\"other\""));
    assert!(!json.contains("nvml"));

    let decoded: GpuMetrics = serde_json::from_str(&json).expect("GPU metrics should deserialize");
    assert_eq!(decoded.current_utilization_pct(), Some(0.0));
    assert_eq!(decoded.current_idle_residency_pct(), Some(100.0));
    assert_eq!(decoded.current_throttle_reasons(), Some(reasons.as_slice()));
}

#[test]
fn legacy_gpu_payloads_keep_new_optional_facts_unknown() {
    let mut value =
        serde_json::to_value(GpuMetrics::default()).expect("default GPU metrics serialize");
    let object = value
        .as_object_mut()
        .expect("GPU metrics serialize as an object");
    object.remove("scalar_observations");
    object.remove("utilization_pct");
    object.remove("idle_residency_pct");

    let decoded: GpuMetrics =
        serde_json::from_value(value).expect("legacy GPU payload should deserialize");

    assert_eq!(decoded.current_utilization_pct(), None);
    assert_eq!(decoded.current_idle_residency_pct(), None);
    assert_eq!(
        decoded.scalar_observations().utilization_pct.availability(),
        ScalarAvailability::Unknown
    );
}

#[test]
fn network_zero_is_current_only_when_typed_observation_proves_it() {
    let mut metrics = NetworkMetrics::default();
    metrics.apply_observations(
        NetworkAdapterType::Unknown,
        NetworkScalarObservations {
            total_rx_bytes: ScalarObservation::available(0, 100),
            total_tx_bytes: ScalarObservation::available(0, 100),
            rx_bytes_per_sec: ScalarObservation::available(0, 100),
            tx_bytes_per_sec: ScalarObservation::available(0, 100),
            utilization_pct: ScalarObservation::available(0.0, 100),
            link_speed_mbps: ScalarObservation::available(1_000, 100),
            link_up: ScalarObservation::available(true, 100),
        },
        NetworkWirelessObservations::default(),
    );

    assert_eq!(metrics.current_total_rx_bytes(), Some(0));
    assert_eq!(metrics.current_total_tx_bytes(), Some(0));
    assert_eq!(metrics.current_rx_bytes_per_sec(), Some(0));
    assert_eq!(metrics.current_tx_bytes_per_sec(), Some(0));
    assert_eq!(metrics.current_utilization_pct(), Some(0.0));
    assert_eq!(metrics.current_link_speed_mbps(), Some(1_000));
    assert_eq!(metrics.current_link_up(), Some(true));
}

#[test]
fn legacy_network_fallback_requires_a_trustworthy_interface_identity() {
    let mut value =
        serde_json::to_value(NetworkMetrics::default()).expect("network metrics serialize");
    let object = value
        .as_object_mut()
        .expect("network metrics serialize as an object");
    object.remove("scalar_observations");
    object.insert("rx_bytes_per_sec".into(), serde_json::json!(0));
    object.insert("tx_bytes_per_sec".into(), serde_json::json!(0));
    object.insert("utilization_pct".into(), serde_json::json!(0.0));

    let default_decoded: NetworkMetrics =
        serde_json::from_value(value.clone()).expect("legacy network metrics deserialize");
    assert_eq!(default_decoded.current_rx_bytes_per_sec(), None);
    assert_eq!(default_decoded.current_tx_bytes_per_sec(), None);
    assert_eq!(default_decoded.current_utilization_pct(), None);

    value
        .as_object_mut()
        .expect("network metrics serialize as an object")
        .insert("interface_name".into(), serde_json::json!("enp1s0"));
    let identified: NetworkMetrics =
        serde_json::from_value(value).expect("identified legacy network metrics deserialize");

    assert_eq!(
        identified
            .scalar_observations()
            .rx_bytes_per_sec
            .availability(),
        ScalarAvailability::Available
    );
    assert_eq!(identified.current_rx_bytes_per_sec(), Some(0));
    assert_eq!(identified.current_tx_bytes_per_sec(), Some(0));
    assert_eq!(identified.current_utilization_pct(), Some(0.0));

    let wired_with_invalid_wireless_projection: NetworkMetrics =
        serde_json::from_value(serde_json::json!({
            "interface_name": "enp1s0",
            "is_wireless": false,
            "signal_dbm": -50,
            "ssid": "must-not-surface"
        }))
        .expect("legacy wired payload remains readable");
    assert_eq!(
        wired_with_invalid_wireless_projection.current_signal_dbm(),
        None
    );
    assert_eq!(wired_with_invalid_wireless_projection.current_ssid(), None);
}

#[test]
fn stale_network_value_keeps_last_success_without_becoming_current() {
    let mut metrics = NetworkMetrics::default();
    metrics.apply_observations(
        NetworkAdapterType::Unknown,
        NetworkScalarObservations {
            rx_bytes_per_sec: ScalarObservation::available(42, 100)
                .transition_failure(FailureKind::PermissionDenied),
            ..Default::default()
        },
        NetworkWirelessObservations::default(),
    );

    assert_eq!(
        metrics
            .scalar_observations()
            .rx_bytes_per_sec
            .availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
    assert_eq!(
        metrics
            .scalar_observations()
            .rx_bytes_per_sec
            .last_known_value(),
        Some(&42)
    );
    assert_eq!(metrics.current_rx_bytes_per_sec(), None);
}

#[test]
fn wireless_optional_truth_distinguishes_wired_unassociated_and_failure() {
    let wired = NetworkWirelessObservations::not_applicable(10);
    let unassociated = NetworkWirelessObservations {
        association: OptionalObservation::absent(20),
        ssid: OptionalObservation::absent(20),
        signal_dbm: OptionalObservation::absent(20),
        ..Default::default()
    };
    let denied = NetworkWirelessObservations::unavailable(FailureKind::PermissionDenied)
        .retain_previous(unassociated.clone());

    assert_eq!(
        wired.association.last_known_state(),
        &OptionalObservationState::NotApplicable
    );
    assert!(unassociated.association.is_current_absent());
    assert_ne!(wired, unassociated);
    assert_eq!(
        denied.association.last_known_state(),
        &OptionalObservationState::Absent,
        "a failure must retain confirmed absence only as stale"
    );
    assert_eq!(
        denied.association.availability(),
        ScalarAvailability::Stale(FailureKind::PermissionDenied)
    );
}

#[test]
fn applying_network_observations_projects_legacy_wire_from_typed_truth() {
    let mut metrics = NetworkMetrics::new("wlan0");
    metrics.device_id = "net:mac:aa:bb".into();
    metrics.apply_observations(
        NetworkAdapterType::WiFi,
        NetworkScalarObservations {
            total_rx_bytes: ScalarObservation::available(10, 100),
            total_tx_bytes: ScalarObservation::available(20, 100),
            rx_bytes_per_sec: ScalarObservation::available(1, 100),
            tx_bytes_per_sec: ScalarObservation::available(2, 100),
            utilization_pct: ScalarObservation::available(3.0, 100),
            link_speed_mbps: ScalarObservation::available(866, 100),
            link_up: ScalarObservation::available(true, 100),
        },
        NetworkWirelessObservations {
            association: OptionalObservation::present(true, 100),
            ssid: OptionalObservation::present("studio".into(), 100),
            signal_dbm: OptionalObservation::present(-55, 100),
            bssid: OptionalObservation::present("02:11:22:33:44:55".into(), 100),
            frequency_mhz: OptionalObservation::present(5220, 100),
            channel: OptionalObservation::present(44, 100),
            rx_bitrate_mbps: OptionalObservation::present(2402, 100),
            tx_bitrate_mbps: OptionalObservation::present(4800, 100),
            protocol: OptionalObservation::present("802.11be (Wi-Fi 7)".into(), 100),
        },
    );

    assert_eq!(metrics.current_total_rx_bytes(), Some(10));
    assert_eq!(metrics.current_total_tx_bytes(), Some(20));
    assert_eq!(metrics.current_rx_bytes_per_sec(), Some(1));
    assert_eq!(metrics.current_tx_bytes_per_sec(), Some(2));
    assert_eq!(metrics.current_link_speed_mbps(), Some(866));
    assert_eq!(metrics.current_ssid(), Some("studio"));
    assert_eq!(metrics.current_signal_dbm(), Some(-55));
    assert_eq!(metrics.current_is_associated(), Some(true));
    assert_eq!(metrics.current_bssid(), Some("02:11:22:33:44:55"));
    assert_eq!(metrics.current_frequency_mhz(), Some(5220));
    assert_eq!(metrics.current_channel(), Some(44));
    assert_eq!(metrics.current_rx_bitrate_mbps(), Some(2402));
    assert_eq!(metrics.current_tx_bitrate_mbps(), Some(4800));
    assert_eq!(metrics.current_protocol(), Some("802.11be (Wi-Fi 7)"));
    let wire = serde_json::to_value(&metrics).expect("serialize network row");
    assert_eq!(wire["is_wireless"], true);
    assert_eq!(wire["ssid"], "studio");
    assert_eq!(wire["rx_bytes_per_sec"], 1);
}

#[test]
fn explicit_typed_adapter_and_failure_win_over_legacy_wireless_fields() {
    let typed_failure = NetworkScalarObservations::unavailable(FailureKind::PermissionDenied);
    let typed_wireless = NetworkWirelessObservations::not_applicable(50);
    let decoded: NetworkMetrics = serde_json::from_value(serde_json::json!({
        "device_id": "network:conflict",
        "interface_name": "virtual0",
        "adapter_type": "Other",
        "is_wireless": true,
        "rx_bytes_per_sec": 99,
        "ssid": "must-not-surface",
        "signal_dbm": -42,
        "scalar_observations": typed_failure,
        "wireless_observations": typed_wireless
    }))
    .expect("decode conflicting payload");

    assert_eq!(decoded.adapter_type(), NetworkAdapterType::Other);
    assert_eq!(decoded.current_rx_bytes_per_sec(), None);
    assert_eq!(decoded.current_ssid(), None);
    assert_eq!(decoded.current_signal_dbm(), None);
    assert_eq!(decoded.current_link_up(), None);
}

#[test]
fn legacy_wifi_hydrates_only_with_identity_and_positive_classification_evidence() {
    let decoded: NetworkMetrics = serde_json::from_value(serde_json::json!({
        "interface_name": "wlan0",
        "is_wireless": true,
        "rx_bytes_per_sec": 0,
        "ssid": "fixture-ap",
        "signal_dbm": -48
    }))
    .expect("decode legacy Wi-Fi row");

    assert_eq!(decoded.adapter_type(), NetworkAdapterType::WiFi);
    assert_eq!(decoded.current_rx_bytes_per_sec(), Some(0));
    assert_eq!(decoded.current_is_associated(), Some(true));
    assert_eq!(decoded.current_ssid(), Some("fixture-ap"));
    assert_eq!(decoded.current_signal_dbm(), Some(-48));
    assert_eq!(
        decoded.current_link_up(),
        None,
        "link state has no legacy fallback"
    );
}

#[test]
fn typed_only_network_payload_roundtrips_without_legacy_success_keys() {
    let mut network = NetworkMetrics::new("eth0");
    network.device_id = "network:typed:eth0".into();
    network.apply_observations(
        NetworkAdapterType::Ethernet,
        NetworkScalarObservations {
            total_rx_bytes: ScalarObservation::available(0, 40),
            total_tx_bytes: ScalarObservation::available(0, 40),
            rx_bytes_per_sec: ScalarObservation::available(0, 40),
            tx_bytes_per_sec: ScalarObservation::available(0, 40),
            utilization_pct: ScalarObservation::available(0.0, 40),
            link_speed_mbps: ScalarObservation::available(0, 40),
            link_up: ScalarObservation::available(false, 40),
        },
        NetworkWirelessObservations::not_applicable(40),
    );
    let mut wire = serde_json::to_value(&network).expect("serialize typed network");
    let object = wire.as_object_mut().expect("network wire object");
    for key in [
        "rx_bytes_per_sec",
        "tx_bytes_per_sec",
        "total_rx_bytes",
        "total_tx_bytes",
        "utilization_pct",
        "link_speed_mbps",
        "is_wireless",
        "ssid",
        "signal_dbm",
    ] {
        object.remove(key);
    }

    let decoded: NetworkMetrics = serde_json::from_value(wire).expect("decode typed-only row");
    assert_eq!(decoded.adapter_type(), NetworkAdapterType::Ethernet);
    assert_eq!(decoded.current_rx_bytes_per_sec(), Some(0));
    assert_eq!(decoded.current_link_up(), Some(false));
    assert_eq!(decoded.current_ssid(), None);
}

#[test]
fn unavailable_network_truth_omits_legacy_success_projection() {
    let mut network = NetworkMetrics::new("unknown0");
    network.device_id = "network:failure".into();
    network.apply_observations(
        NetworkAdapterType::Unknown,
        NetworkScalarObservations::unavailable(FailureKind::PermissionDenied),
        NetworkWirelessObservations::unavailable(FailureKind::PermissionDenied),
    );

    let wire = serde_json::to_value(network).expect("serialize network failure");
    for key in [
        "rx_bytes_per_sec",
        "tx_bytes_per_sec",
        "total_rx_bytes",
        "total_tx_bytes",
        "utilization_pct",
        "link_speed_mbps",
        "is_wireless",
        "ssid",
        "signal_dbm",
    ] {
        assert!(
            wire.get(key).is_none(),
            "failure must omit legacy key {key}"
        );
    }
}
