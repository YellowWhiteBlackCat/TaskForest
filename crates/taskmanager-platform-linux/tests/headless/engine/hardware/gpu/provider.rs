use std::time::Instant;

use taskmanager_core::{
    DeviceId, DeviceRefreshOutcome, DeviceState, DeviceStatus, FailureKind, GpuMetricField,
    GpuMetrics, GpuScalarObservations, ProviderId, ScalarAvailability, ScalarObservation,
};

use super::super::{GpuProviderFieldFailure, GpuProviderSample, probe_amdgpu_device};
use super::amd::{AMD_SYSFS_PROVIDER_ID, AmdSysfsGpuProvider};
use super::drm::{DRM_PROVIDER_ID, DrmSysfsGpuProvider};
use super::intel::{INTEL_SYSFS_PROVIDER_ID, IntelSysfsGpuProvider};
use super::nvidia::NVIDIA_PROCFS_PROVIDER_ID;
#[cfg(feature = "nvidia")]
use super::nvidia::NVML_PROVIDER_ID;
use super::{GpuProviderFailure, GpuProviderRegistry, GpuTelemetryProvider};

struct FixtureProvider {
    id: &'static str,
    priority: u16,
    result: Result<Vec<GpuProviderSample>, DeviceStatus>,
}

impl GpuTelemetryProvider for FixtureProvider {
    fn id(&self) -> ProviderId {
        ProviderId::borrowed(self.id)
    }

    fn priority(&self) -> u16 {
        self.priority
    }

    fn collect(&mut self, _now: Instant) -> Result<Vec<GpuProviderSample>, GpuProviderFailure> {
        self.result.clone().map_err(GpuProviderFailure::new)
    }
}

fn sample(id: &str, brand: &str, usage: f32, fields: &[GpuMetricField]) -> GpuProviderSample {
    let mut metrics = GpuMetrics::new(id, brand);
    metrics.device_state = DeviceState::healthy(1);
    metrics.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(usage, 1),
        ..Default::default()
    });
    GpuProviderSample {
        metrics,
        fields: fields.to_vec(),
        field_failures: Vec::new(),
    }
}

fn registry(providers: impl IntoIterator<Item = FixtureProvider>) -> GpuProviderRegistry {
    let mut registry = GpuProviderRegistry {
        entries: Vec::new(),
        scalar_tracker: Default::default(),
    };
    for provider in providers {
        registry.register(provider);
    }
    registry
}

#[test]
fn same_pci_identity_merges_by_priority_with_per_field_provenance() {
    let baseline = FixtureProvider {
        id: "fixture.baseline",
        priority: 10,
        result: Ok(vec![sample(
            "gpu:pci:0000:01:00.0",
            "NVIDIA",
            0.0,
            &[GpuMetricField::Identity, GpuMetricField::Brand],
        )]),
    };
    let enrichment = FixtureProvider {
        id: "fixture.enrichment",
        priority: 100,
        result: Ok(vec![sample(
            "gpu:pci:0000:01:00.0",
            "NVIDIA RTX",
            42.0,
            &[GpuMetricField::Brand, GpuMetricField::Utilization],
        )]),
    };
    let mut registry = registry([enrichment, baseline]);

    let snapshot = registry.collect(Instant::now(), 50);

    assert_eq!(snapshot.metrics.len(), 1);
    assert_eq!(snapshot.metrics[0].brand, "NVIDIA RTX");
    assert_eq!(snapshot.metrics[0].current_utilization_pct(), Some(42.0));
    assert_eq!(
        snapshot.metrics[0]
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::Utilization)
            .map(|item| item.provider.as_str()),
        Some("fixture.enrichment")
    );
}

#[test]
fn equal_models_on_distinct_pci_identities_remain_distinct() {
    let mut registry = registry([FixtureProvider {
        id: "fixture.baseline",
        priority: 10,
        result: Ok(vec![
            sample(
                "gpu:pci:0000:01:00.0",
                "Same Model",
                1.0,
                &[GpuMetricField::Identity, GpuMetricField::Brand],
            ),
            sample(
                "gpu:pci:0000:02:00.0",
                "Same Model",
                2.0,
                &[GpuMetricField::Identity, GpuMetricField::Brand],
            ),
        ]),
    }]);

    let snapshot = registry.collect(Instant::now(), 50);

    assert_eq!(snapshot.metrics.len(), 2);
    assert_ne!(snapshot.metrics[0].device_id, snapshot.metrics[1].device_id);
}

#[test]
fn failed_enrichment_preserves_baseline_and_reports_partial_state() {
    let mut registry = registry([
        FixtureProvider {
            id: "fixture.enrichment",
            priority: 100,
            result: Err(DeviceStatus::MissingTool),
        },
        FixtureProvider {
            id: "fixture.baseline",
            priority: 10,
            result: Ok(vec![sample(
                "gpu:pci:0000:01:00.0",
                "Baseline",
                0.0,
                &[GpuMetricField::Identity, GpuMetricField::Brand],
            )]),
        },
    ]);

    let snapshot = registry.collect(Instant::now(), 50);

    assert_eq!(snapshot.metrics.len(), 1);
    assert_eq!(snapshot.metrics[0].brand, "Baseline");
    assert!(snapshot.provider_states.iter().any(|state| {
        state.provider.as_str() == "fixture.enrichment"
            && state.status == DeviceStatus::MissingTool
            && state.last_success_ms.is_none()
    }));
    assert!(snapshot.sources.iter().any(|source| {
        source.provider.as_str() == "fixture.enrichment"
            && source.outcome
                == taskmanager_core::SourceOutcome::Unavailable(
                    taskmanager_core::FailureKind::MissingDependency,
                )
            && source.item_count == 0
    }));
}

#[test]
fn partial_runtime_field_receipt_maps_exact_failure_without_hiding_the_device() {
    let device_id = "gpu:pci:0000:01:00.0";
    let mut partial = sample(
        device_id,
        "Runtime GPU",
        0.0,
        &[GpuMetricField::Identity, GpuMetricField::Brand],
    );
    partial
        .metrics
        .apply_scalar_observations(GpuScalarObservations::default());
    partial.field_failures.push(GpuProviderFieldFailure {
        field: GpuMetricField::Utilization,
        failure: FailureKind::PermissionDenied,
    });
    let mut registry = registry([FixtureProvider {
        id: "fixture.runtime",
        priority: 100,
        result: Ok(vec![partial]),
    }]);

    let snapshot = registry.collect(Instant::now(), 50);

    assert_eq!(snapshot.metrics.len(), 1);
    assert_eq!(snapshot.metrics[0].device_id, device_id);
    assert_eq!(
        snapshot.metrics[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::PermissionDenied)
    );
    assert_eq!(snapshot.metrics[0].current_utilization_pct(), None);
}

#[test]
fn registration_order_does_not_change_merge_result() {
    fn providers() -> [FixtureProvider; 2] {
        [
            FixtureProvider {
                id: "fixture.low",
                priority: 10,
                result: Ok(vec![sample(
                    "gpu:pci:0000:01:00.0",
                    "Low",
                    10.0,
                    &[GpuMetricField::Identity, GpuMetricField::Brand],
                )]),
            },
            FixtureProvider {
                id: "fixture.high",
                priority: 20,
                result: Ok(vec![sample(
                    "gpu:pci:0000:01:00.0",
                    "High",
                    20.0,
                    &[GpuMetricField::Brand, GpuMetricField::Utilization],
                )]),
            },
        ]
    }
    let [left_low, left_high] = providers();
    let [right_low, right_high] = providers();
    let mut left = registry([left_low, left_high]);
    let mut right = registry([right_high, right_low]);

    let left = left.collect(Instant::now(), 50);
    let right = right.collect(Instant::now(), 50);

    assert_eq!(left.metrics[0].brand, right.metrics[0].brand);
    assert_eq!(
        left.metrics[0].current_utilization_pct(),
        right.metrics[0].current_utilization_pct()
    );
    assert_eq!(left.metrics[0].provenance, right.metrics[0].provenance);
}

#[test]
fn standard_registry_contains_generic_and_all_compiled_enhancement_providers() {
    let registry = GpuProviderRegistry::standard();
    let ids = registry
        .entries
        .iter()
        .map(|entry| entry.provider.id())
        .collect::<Vec<_>>();

    assert!(ids.iter().any(|id| id == &DRM_PROVIDER_ID));
    assert!(ids.iter().any(|id| id == &AMD_SYSFS_PROVIDER_ID));
    assert!(ids.iter().any(|id| id == &INTEL_SYSFS_PROVIDER_ID));
    assert!(ids.iter().any(|id| id == &NVIDIA_PROCFS_PROVIDER_ID));
    #[cfg(feature = "nvidia")]
    assert!(ids.iter().any(|id| id == &NVML_PROVIDER_ID));
}

/// A module-declared driver version is a DRM identity fact: the authoritative
/// DRM provider owns both the value and its provenance receipt, so the merged
/// row credits `linux.gpu.drm-sysfs` for `DriverVersion` exactly as it does
/// for the driver name.
#[cfg(unix)]
#[test]
fn drm_provider_owns_module_declared_driver_version() {
    let base = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_provider_modver_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let root = base.join("drm");
    let module_root = base.join("module");
    let device = root.join("card0").join("device");
    let module_dir = module_root.join("nvidia");
    std::fs::create_dir_all(&module_dir).expect("fixture module directory");
    std::fs::create_dir_all(&device).expect("fixture device directory");
    std::fs::write(device.join("vendor"), "0x10de\n").expect("vendor node");
    std::fs::write(device.join("uevent"), "PCI_SLOT_NAME=0000:01:00.0\n").expect("slot node");
    std::fs::write(module_dir.join("version"), "550.90.07\n").expect("module version node");
    // The `driver` symlink's basename names the kernel driver.
    std::os::unix::fs::symlink(&module_dir, device.join("driver")).expect("driver symlink");

    let mut registry = GpuProviderRegistry {
        entries: Vec::new(),
        scalar_tracker: Default::default(),
    };
    registry.register(DrmSysfsGpuProvider::new(root, module_root));

    let snapshot = registry.collect(Instant::now(), 10);
    assert_eq!(snapshot.metrics.len(), 1);
    let metric = &snapshot.metrics[0];
    assert_eq!(metric.driver.as_deref(), Some("nvidia"));
    assert_eq!(metric.driver_version.as_deref(), Some("550.90.07"));
    assert_eq!(
        metric
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::DriverVersion)
            .map(|item| item.provider.as_str()),
        Some(DRM_PROVIDER_ID.as_str())
    );

    std::fs::remove_dir_all(&base).ok();
}

#[test]
fn amd_runtime_nodes_enrich_generic_drm_identity_with_field_provenance() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_provider_amd_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let device = root.join("card0/device");
    let hwmon = device.join("hwmon/hwmon0");
    std::fs::create_dir_all(&hwmon).unwrap();
    for (name, value) in [
        ("vendor", "0x1002\n"),
        ("uevent", "PCI_SLOT_NAME=0000:03:00.0\n"),
        ("gpu_busy_percent", "61\n"),
        ("gfx_busy_percent", "52\n"),
        ("future_media_busy_percent", "7\n"),
        ("mem_info_vram_used", "1024\n"),
        ("mem_info_vram_total", "4096\n"),
        ("mem_info_gtt_used", "256\n"),
        ("mem_info_gtt_total", "2048\n"),
        ("pp_dpm_sclk", "0: 500Mhz\n1: 1800Mhz *\n2: 2400Mhz\n"),
        ("throttle_reason_status", "0x00000000\n"),
    ] {
        std::fs::write(device.join(name), value).unwrap();
    }
    for (name, value) in [
        ("temp1_input", "65000\n"),
        ("power1_average", "125000000\n"),
        ("fan1_input", "1700\n"),
        ("pwm1", "128\n"),
        ("pwm1_max", "255\n"),
    ] {
        std::fs::write(hwmon.join(name), value).unwrap();
    }

    let mut registry = GpuProviderRegistry {
        entries: Vec::new(),
        scalar_tracker: Default::default(),
    };
    registry.register(DrmSysfsGpuProvider::new(
        root.clone(),
        root.join("absent_module_root"),
    ));
    registry.register(AmdSysfsGpuProvider::new(root.clone()));

    let snapshot = registry.collect(Instant::now(), 500);
    assert_eq!(
        snapshot.authoritative_refresh,
        Some(DeviceRefreshOutcome::Complete)
    );
    assert_eq!(snapshot.metrics.len(), 1);
    let metric = &snapshot.metrics[0];
    assert_eq!(metric.device_id, "gpu:pci:0000:03:00.0");
    assert_eq!(metric.brand, "AMD");
    assert_eq!(metric.current_utilization_pct(), Some(61.0));
    assert_eq!(metric.current_memory_used_bytes(), Some(1_280));
    assert_eq!(metric.current_memory_total_bytes(), Some(6_144));
    assert_eq!(metric.current_frequency_mhz(), Some(1_800));
    assert_eq!(metric.current_max_frequency_mhz(), Some(2_400));
    assert_eq!(metric.current_fan_speed_rpm(), Some(1_700));
    assert!(
        metric
            .current_fan_speed_pct()
            .is_some_and(|value| value > 50.0)
    );
    assert_eq!(metric.current_utilization_pct(), Some(61.0));
    assert_eq!(metric.current_memory_used_bytes(), Some(1_280));
    assert_eq!(metric.current_memory_total_bytes(), Some(6_144));
    assert_eq!(metric.current_frequency_mhz(), Some(1_800));
    assert_eq!(metric.current_max_frequency_mhz(), Some(2_400));
    assert_eq!(metric.current_fan_speed_rpm(), Some(1_700));
    assert_eq!(metric.current_power_w(), Some(125.0));
    assert_eq!(
        metric
            .scalar_observations()
            .utilization_pct
            .last_success_ms(),
        Some(500)
    );
    assert_eq!(metric.current_throttle_reasons(), Some([].as_slice()));
    assert!(
        metric
            .engines
            .iter()
            .any(|engine| engine.name == "FUTURE MEDIA")
    );

    for field in [
        GpuMetricField::Utilization,
        GpuMetricField::Memory,
        GpuMetricField::Engines,
        GpuMetricField::Temperature,
        GpuMetricField::Power,
        GpuMetricField::Fan,
        GpuMetricField::Frequency,
        GpuMetricField::Throttle,
    ] {
        assert_eq!(
            metric
                .provenance
                .iter()
                .find(|item| item.field == field)
                .map(|item| item.provider.as_str()),
            Some(AMD_SYSFS_PROVIDER_ID.as_str()),
            "wrong AMD provenance for {field:?}"
        );
    }
    assert_eq!(
        metric
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::Identity)
            .map(|item| item.provider.as_str()),
        Some(DRM_PROVIDER_ID.as_str())
    );
    assert!(snapshot.provider_states.iter().all(|state| {
        state.status == DeviceStatus::Healthy && state.last_success_ms == Some(500)
    }));

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn amd_partial_field_failures_are_exact_and_successful_siblings_remain_current() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_provider_amd_partial_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let device = root.join("card0/device");
    let hwmon = device.join("hwmon/hwmon0");
    std::fs::create_dir_all(&hwmon).expect("fixture hwmon directory");
    for (name, value) in [
        ("vendor", "0x1002\n"),
        ("uevent", "PCI_SLOT_NAME=0000:05:00.0\n"),
        ("gpu_busy_percent", "not-a-percent\n"),
        ("gfx_busy_percent", "broken\n"),
        ("future_media_busy_percent", "9\n"),
        ("mem_info_vram_used", "2048\n"),
        ("pp_dpm_sclk", "broken clock table\n"),
        ("throttle_reason_status", "not-a-bitmask\n"),
    ] {
        std::fs::write(device.join(name), value).expect("fixture device node");
    }
    for (name, value) in [
        ("temp1_input", "invalid\n"),
        ("power1_average", "125000000\n"),
        ("fan1_input", "invalid\n"),
        ("pwm1", "128\n"),
        ("pwm1_max", "255\n"),
        ("freq1_input", "1800000000\n"),
    ] {
        std::fs::write(hwmon.join(name), value).expect("fixture hwmon node");
    }

    let probe = probe_amdgpu_device("card0", &device);
    let sample = probe.sample.expect("runtime-selected AMD sample");
    assert!(
        sample
            .metrics
            .engines
            .iter()
            .any(|engine| engine.name == "FUTURE MEDIA")
    );
    for field in [
        GpuMetricField::Utilization,
        GpuMetricField::Engines,
        GpuMetricField::Temperature,
        GpuMetricField::Fan,
        GpuMetricField::Frequency,
        GpuMetricField::Throttle,
    ] {
        assert!(
            sample.field_failures.contains(&GpuProviderFieldFailure {
                field,
                failure: FailureKind::ProviderFault,
            }),
            "missing exact provider-fault receipt for {field:?}"
        );
    }
    assert!(sample.field_failures.contains(&GpuProviderFieldFailure {
        field: GpuMetricField::Memory,
        failure: FailureKind::Unsupported,
    }));

    let mut registry = GpuProviderRegistry {
        entries: Vec::new(),
        scalar_tracker: Default::default(),
    };
    registry.register(DrmSysfsGpuProvider::new(
        root.clone(),
        root.join("absent_module_root"),
    ));
    registry.register(AmdSysfsGpuProvider::new(root.clone()));
    let snapshot = registry.collect(Instant::now(), 600);
    let metric = &snapshot.metrics[0];

    assert_eq!(metric.current_utilization_pct(), None);
    assert_eq!(
        metric.scalar_observations().utilization_pct.availability(),
        ScalarAvailability::Unavailable(FailureKind::ProviderFault)
    );
    assert_eq!(
        metric
            .scalar_observations()
            .idle_residency_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::Unsupported)
    );
    assert_eq!(metric.current_memory_used_bytes(), Some(2_048));
    assert_eq!(
        metric
            .scalar_observations()
            .memory_used_bytes
            .availability(),
        ScalarAvailability::Partial(FailureKind::Unsupported)
    );
    assert_eq!(metric.current_power_w(), Some(125.0));
    assert_eq!(metric.current_fan_speed_pct().map(f32::round), Some(50.0));
    assert_eq!(
        metric.scalar_observations().fan_speed_pct.availability(),
        ScalarAvailability::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(metric.current_frequency_mhz(), Some(1_800));
    assert_eq!(
        metric.scalar_observations().frequency_mhz.availability(),
        ScalarAvailability::Partial(FailureKind::ProviderFault)
    );
    assert_eq!(metric.current_temperature_c(), None);
    assert_eq!(metric.current_max_frequency_mhz(), None);

    std::fs::remove_dir_all(root).ok();
}

/// ADR-015: an AMD `mem_info` partial read must never collapse a missing
/// sibling into a believable zero. When `mem_info_vram_used` is readable but
/// `mem_info_vram_total` (and both GTT nodes) are absent, the unread total
/// must surface as a typed absent value plus a `GpuMetricField::Memory`
/// failure — not as `dedicated_vram_total_bytes == 0` / `memory_total_bytes ==
/// Some(0)`, which would falsely report "0 bytes total VRAM" and let used
/// exceed total. The successfully read `used` value is still advertised.
#[test]
fn amd_partial_vram_read_does_not_collapse_missing_total_to_zero() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_provider_amd_partial_vram_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let device = root.join("card0/device");
    std::fs::create_dir_all(&device).expect("fixture device directory");
    for (name, value) in [
        ("vendor", "0x1002\n"),
        ("uevent", "PCI_SLOT_NAME=0000:07:00.0\n"),
        // Only the dedicated-used node is present; total + both GTT nodes are
        // absent (the partial-VRAM failure mode).
        ("mem_info_vram_used", "2048\n"),
    ] {
        std::fs::write(device.join(name), value).expect("fixture device node");
    }

    let probe = probe_amdgpu_device("card0", &device);
    let sample = probe.sample.expect("runtime-selected AMD sample");
    let metrics = &sample.metrics;

    // The one real reading is advertised through every layer.
    assert_eq!(metrics.current_dedicated_vram_used_bytes(), Some(2_048));
    assert_eq!(metrics.current_memory_used_bytes(), Some(2_048));
    assert!(
        sample.fields.contains(&GpuMetricField::Memory),
        "Memory must still be advertised because used was read: {:?}",
        sample.fields
    );

    // The unread total is NOT reported as a believable zero. The typed Option
    // is absent rather than Some(0), and the missing sysfs nodes are accounted
    // for by an exact Memory field failure (per-node, via the failure channel).
    assert_eq!(
        metrics.current_memory_total_bytes(),
        None,
        "missing total must be typed-absent, not Some(0)"
    );
    assert!(
        sample.field_failures.contains(&GpuProviderFieldFailure {
            field: GpuMetricField::Memory,
            failure: FailureKind::Unsupported,
        }),
        "missing mem_info_* nodes must produce a Memory field failure: {:?}",
        sample.field_failures
    );

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn amd_field_failure_retains_only_its_prior_value_and_then_recovers() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_provider_amd_recovery_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after epoch")
            .as_nanos()
    ));
    let device = root.join("card0/device");
    std::fs::create_dir_all(&device).expect("fixture device directory");
    for (name, value) in [
        ("vendor", "0x1002\n"),
        ("uevent", "PCI_SLOT_NAME=0000:06:00.0\n"),
        ("gpu_busy_percent", "40\n"),
    ] {
        std::fs::write(device.join(name), value).expect("fixture device node");
    }
    let mut registry = GpuProviderRegistry {
        entries: Vec::new(),
        scalar_tracker: Default::default(),
    };
    registry.register(DrmSysfsGpuProvider::new(
        root.clone(),
        root.join("absent_module_root"),
    ));
    registry.register(AmdSysfsGpuProvider::new(root.clone()));
    let started_at = Instant::now();

    let first = registry.collect(started_at, 10);
    assert_eq!(first.metrics[0].current_utilization_pct(), Some(40.0));

    std::fs::write(device.join("gpu_busy_percent"), "bad\n").expect("break utilization");
    let failed = registry.collect(started_at, 20);
    assert_eq!(failed.metrics[0].current_utilization_pct(), None);
    assert_eq!(
        failed.metrics[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Stale(FailureKind::ProviderFault)
    );
    assert_eq!(
        failed.metrics[0]
            .scalar_observations()
            .utilization_pct
            .last_known_value(),
        Some(&40.0)
    );

    std::fs::write(device.join("gpu_busy_percent"), "0\n").expect("recover utilization");
    let recovered = registry.collect(started_at, 30);
    assert_eq!(recovered.metrics[0].current_utilization_pct(), Some(0.0));
    assert_eq!(
        recovered.metrics[0]
            .scalar_observations()
            .utilization_pct
            .last_success_ms(),
        Some(30)
    );

    std::fs::remove_dir_all(root).ok();
}

#[test]
fn intel_runtime_provider_owns_frequency_and_rc6_not_drm_inventory() {
    let root = crate::test_support::repo_temp_dir().join(format!(
        "tm_gpu_provider_intel_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let device = root.join("card0/device");
    let frequency = device.join("tile0/gt0/freq0");
    let idle = device.join("tile0/gt0/gtidle/idle_residency_ms");
    std::fs::create_dir_all(&frequency).unwrap();
    std::fs::create_dir_all(idle.parent().unwrap()).unwrap();
    std::fs::write(device.join("vendor"), "0x8086\n").unwrap();
    std::fs::write(device.join("uevent"), "PCI_SLOT_NAME=0000:00:02.0\n").unwrap();
    std::fs::write(frequency.join("act_freq"), "900\n").unwrap();
    std::fs::write(frequency.join("max_freq"), "2200\n").unwrap();
    std::fs::write(&idle, "0\n").unwrap();

    let mut registry = GpuProviderRegistry {
        entries: Vec::new(),
        scalar_tracker: Default::default(),
    };
    registry.register(DrmSysfsGpuProvider::new(
        root.clone(),
        root.join("absent_module_root"),
    ));
    registry.register(IntelSysfsGpuProvider::new(
        root.clone(),
        root.join("absent_module_root"),
    ));
    let started_at = Instant::now();

    let first = registry.collect(started_at, 100);
    let first_metric = &first.metrics[0];
    assert_eq!(first_metric.current_frequency_mhz(), Some(900));
    assert_eq!(first_metric.current_max_frequency_mhz(), Some(2_200));
    assert_eq!(first_metric.current_frequency_mhz(), Some(900));
    assert_eq!(first_metric.current_max_frequency_mhz(), Some(2_200));
    assert_eq!(
        first_metric
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::Frequency)
            .map(|item| item.provider.as_str()),
        Some(INTEL_SYSFS_PROVIDER_ID.as_str())
    );

    std::fs::write(&idle, "250\n").unwrap();
    let second = registry.collect(started_at + std::time::Duration::from_secs(1), 200);
    let second_metric = &second.metrics[0];
    assert_eq!(second_metric.current_idle_residency_pct(), Some(25.0));
    assert_eq!(second_metric.current_utilization_pct(), Some(75.0));
    assert_eq!(second_metric.current_utilization_pct(), Some(75.0));
    assert_eq!(second_metric.current_idle_residency_pct(), Some(25.0));
    assert_eq!(
        second_metric
            .scalar_observations()
            .idle_residency_pct
            .last_success_ms(),
        Some(200)
    );
    assert_eq!(
        second_metric
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::Utilization)
            .map(|item| item.provider.as_str()),
        Some(INTEL_SYSFS_PROVIDER_ID.as_str())
    );
    assert_eq!(
        second_metric
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::IdleResidency)
            .map(|item| item.provider.as_str()),
        Some(INTEL_SYSFS_PROVIDER_ID.as_str())
    );
    assert_eq!(
        second_metric
            .provenance
            .iter()
            .find(|item| item.field == GpuMetricField::Identity)
            .map(|item| item.provider.as_str()),
        Some(DRM_PROVIDER_ID.as_str())
    );

    std::fs::write(&idle, "100\n").expect("simulate RC6 counter reset");
    let reset = registry.collect(started_at + std::time::Duration::from_secs(2), 300);
    assert_eq!(reset.metrics[0].current_utilization_pct(), None);
    assert_eq!(
        reset.metrics[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Stale(FailureKind::IdentityChanged)
    );
    assert_eq!(
        reset.metrics[0]
            .scalar_observations()
            .utilization_pct
            .last_known_value(),
        Some(&75.0)
    );

    std::fs::write(&idle, "200\n").expect("recover RC6 counter");
    let recovered = registry.collect(started_at + std::time::Duration::from_secs(3), 400);
    assert_eq!(recovered.metrics[0].current_utilization_pct(), Some(90.0));

    let device_id = DeviceId::new("gpu:pci:0000:00:02.0");
    registry.prune_generations(std::slice::from_ref(&device_id));
    std::fs::write(&idle, "300\n").expect("new generation RC6 baseline");
    let readded = registry.collect(started_at + std::time::Duration::from_secs(4), 500);
    assert_eq!(readded.metrics[0].current_utilization_pct(), None);
    assert_eq!(
        readded.metrics[0]
            .scalar_observations()
            .utilization_pct
            .availability(),
        ScalarAvailability::Unavailable(FailureKind::TemporarilyUnavailable)
    );
    assert_eq!(
        readded.metrics[0]
            .scalar_observations()
            .utilization_pct
            .last_known_value(),
        None
    );

    std::fs::write(&idle, "400\n").expect("new generation RC6 recovery");
    let readded_recovered = registry.collect(started_at + std::time::Duration::from_secs(5), 600);
    assert_eq!(
        readded_recovered.metrics[0].current_utilization_pct(),
        Some(90.0)
    );

    std::fs::remove_dir_all(root).ok();
}
