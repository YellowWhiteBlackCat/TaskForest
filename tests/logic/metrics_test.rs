use taskmanager::core::metrics::{
    CpuMetrics, CpuScalarObservations, DiskMetrics, GpuMetrics, GpuScalarObservations,
    MemoryCompositionObservations, MemoryMetrics, MemoryModuleObservations,
    MemoryOptionalObservations, MemoryScalarObservations, OptionalObservation, ScalarObservation,
    SmartAvailability, StorageConnection, StorageDeviceKind, StorageInterconnect, StorageProtocol,
    SystemSnapshot,
};
// The sysfs-path classifiers below are Linux-provider helpers; the rest of
// this module tests platform-neutral core metrics math and must keep running
// on every OS.
#[cfg(target_os = "linux")]
use std::fs;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use std::time::SystemTime;
#[cfg(target_os = "linux")]
use taskmanager_platform_linux::{detect_gpu_metrics_from_paths, is_virtual_interface};

#[test]
fn test_memory_metrics_calculation() {
    let mem = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(16 * 1024 * 1024 * 1024, 1),
            used_bytes: ScalarObservation::available(8 * 1024 * 1024 * 1024, 1),
            available_bytes: ScalarObservation::available(8 * 1024 * 1024 * 1024, 1),
            swap_total_bytes: ScalarObservation::available(4 * 1024 * 1024 * 1024, 1),
            swap_used_bytes: ScalarObservation::available(1024 * 1024 * 1024, 1),
            ..Default::default()
        },
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                cached_bytes: OptionalObservation::present(4 * 1024 * 1024 * 1024, 1),
                buffers_bytes: OptionalObservation::present(512 * 1024 * 1024, 1),
                ..Default::default()
            },
            hardware_reserved_bytes: OptionalObservation::present(0, 1),
            modules: MemoryModuleObservations {
                speed_mhz: OptionalObservation::present(3_200, 1),
                slots_used: OptionalObservation::present(2, 1),
                slots_total: OptionalObservation::present(4, 1),
                ..Default::default()
            },
            ..Default::default()
        },
    );

    assert_eq!(mem.used_percentage_observed(), Some(50.0));
    assert_eq!(mem.swap_percentage_observed(), Some(25.0));
    assert_eq!(mem.current_speed_mhz(), Some(3_200));
    assert_eq!(mem.current_slots_used(), Some(2));
    assert_eq!(mem.current_slots_total(), Some(4));
}

#[test]
fn test_memory_metrics_zero_total_guards() {
    // An observed zero denominator stays distinct from an unknown one and
    // never enters division.
    let no_mem = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(0, 1),
            used_bytes: ScalarObservation::available(0, 1),
            swap_total_bytes: ScalarObservation::available(0, 1),
            swap_used_bytes: ScalarObservation::available(0, 1),
            ..Default::default()
        },
        Default::default(),
    );
    assert_eq!(no_mem.used_percentage_observed(), None);
    assert_eq!(no_mem.swap_percentage_observed(), None);

    // Even with used > 0 but total == 0, the guard still returns 0.0 (would
    // otherwise divide by zero / produce inf/NaN).
    let inconsistent = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(0, 1),
            used_bytes: ScalarObservation::available(500, 1),
            swap_total_bytes: ScalarObservation::available(0, 1),
            swap_used_bytes: ScalarObservation::available(100, 1),
            ..Default::default()
        },
        Default::default(),
    );
    assert_eq!(inconsistent.used_percentage_observed(), None);
    assert_eq!(inconsistent.swap_percentage_observed(), None);

    // The no-swap host: real memory totals but swap_total == 0. swap_percentage
    // must report 0.0 rather than dividing by zero.
    let no_swap = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(16 * 1024 * 1024 * 1024, 1),
            used_bytes: ScalarObservation::available(8 * 1024 * 1024 * 1024, 1),
            swap_total_bytes: ScalarObservation::available(0, 1),
            swap_used_bytes: ScalarObservation::available(0, 1),
            ..Default::default()
        },
        Default::default(),
    );
    assert_eq!(no_swap.used_percentage_observed(), Some(50.0));
    assert_eq!(no_swap.swap_percentage_observed(), None);
}

#[cfg(target_os = "linux")]
#[test]
fn test_virtual_interface_filtering() {
    assert!(is_virtual_interface("docker0"));
    assert!(is_virtual_interface("veth1234"));
    assert!(is_virtual_interface("br-5678a"));
    assert!(is_virtual_interface("virbr0"));
    assert!(is_virtual_interface("vnet0"));
    assert!(is_virtual_interface("tun0"));
    assert!(is_virtual_interface("tap0"));
    assert!(is_virtual_interface("lo"));

    assert!(!is_virtual_interface("eth0"));
    assert!(!is_virtual_interface("wlan0"));
    assert!(!is_virtual_interface("enp0s31f6"));
}

#[cfg(target_os = "linux")]
fn create_temp_test_dir(prefix: &str) -> PathBuf {
    let mut dir = crate::test_support::repo_temp_dir();
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    dir.push(format!("{}_{}", prefix, nanos));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(target_os = "linux")]
#[test]
fn test_gpu_metrics_scanning_mock() {
    let root = create_temp_test_dir("gpu_mock");
    let drm_dir = root.join("drm");
    let nvidia_dir = root.join("nvidia");

    let card0_device = drm_dir.join("card0").join("device");
    fs::create_dir_all(&card0_device).unwrap();

    fs::write(card0_device.join("gpu_busy_percent"), "75\n").unwrap();
    fs::write(card0_device.join("mem_info_vram_used"), "4294967296\n").unwrap();
    fs::write(card0_device.join("mem_info_vram_total"), "17179869184\n").unwrap();
    fs::write(card0_device.join("vendor"), "0x1002\n").unwrap();

    let hwmon = card0_device.join("hwmon").join("hwmon0");
    fs::create_dir_all(&hwmon).unwrap();
    fs::write(hwmon.join("temp1_input"), "62000\n").unwrap();

    let gpus = detect_gpu_metrics_from_paths(&drm_dir, &nvidia_dir);
    assert_eq!(gpus.len(), 1);
    let gpu = &gpus[0];
    assert_eq!(gpu.brand, "AMD");
    assert_eq!(gpu.current_utilization_pct(), Some(75.0));
    assert_eq!(gpu.current_dedicated_vram_used_bytes(), Some(4_294_967_296));
    assert_eq!(
        gpu.current_dedicated_vram_total_bytes(),
        Some(17_179_869_184)
    );
    assert_eq!(gpu.current_temperature_c(), Some(62.0));

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn test_system_snapshot_with_gpu() {
    let mut gpu = GpuMetrics::from_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(35.5, 1_000),
        temperature_c: ScalarObservation::available(48.0, 1_000),
        dedicated_vram_used_bytes: ScalarObservation::available(3_221_225_472, 1_000),
        dedicated_vram_total_bytes: ScalarObservation::available(17_179_869_184, 1_000),
        ..Default::default()
    });
    gpu.brand = "NVIDIA GeForce RTX 4080".to_string();

    let mut cpu = CpuMetrics::from_observations(CpuScalarObservations {
        global_usage_pct: ScalarObservation::available(12.0, 1_000),
        core_usage_group: taskmanager::core::metrics::ScalarObservationGroup::available(
            vec![12.0],
            1_000,
        ),
        frequency_mhz: ScalarObservation::available(4_500, 1_000),
        ..Default::default()
    });
    cpu.brand = Some("AMD Ryzen 9".to_string());
    cpu.physical_cores = Some(8);
    cpu.logical_cores = Some(16);
    cpu.l1_cache_kb = Some(512);
    cpu.l2_cache_kb = Some(8_192);
    cpu.l3_cache_kb = Some(32_768);
    let memory = MemoryMetrics::from_observations(
        MemoryScalarObservations {
            total_bytes: ScalarObservation::available(34_359_738_368, 1_000),
            used_bytes: ScalarObservation::available(17_179_869_184, 1_000),
            available_bytes: ScalarObservation::available(17_179_869_184, 1_000),
            swap_total_bytes: ScalarObservation::available(0, 1_000),
            swap_used_bytes: ScalarObservation::available(0, 1_000),
            ..Default::default()
        },
        MemoryOptionalObservations {
            composition: MemoryCompositionObservations {
                cached_bytes: OptionalObservation::present(4_294_967_296, 1_000),
                buffers_bytes: OptionalObservation::present(1_073_741_824, 1_000),
                ..Default::default()
            },
            hardware_reserved_bytes: OptionalObservation::present(0, 1_000),
            modules: MemoryModuleObservations {
                speed_mhz: OptionalObservation::present(6_000, 1_000),
                slots_used: OptionalObservation::present(2, 1_000),
                slots_total: OptionalObservation::present(4, 1_000),
                ..Default::default()
            },
            ..Default::default()
        },
    );
    let snapshot = SystemSnapshot {
        timestamp_ms: 1000,
        uptime_secs: 0,
        processes: 0,
        threads: Some(0),
        cpu,
        memory,
        disks: vec![],
        networks: vec![],
        gpu: vec![gpu],
        telemetry_sources: vec![],
        provider_states: vec![],
        device_lifecycles: Default::default(),
    };

    assert_eq!(snapshot.gpu.len(), 1);
    assert_eq!(snapshot.gpu[0].brand, "NVIDIA GeForce RTX 4080");
}

#[test]
fn test_disk_metrics_smart_fields_reported() {
    let d = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .name("nvme0n1".to_string())
        .smart_availability(SmartAvailability::Available)
        .smart_temperature_c(Some(42.5))
        .smart_critical_warning(Some(false))
        .smart_temp_critical_c(Some(85.0))
        .smart_percent_used(Some(15.0))
        .smart_power_on_hours(Some(8760))
        .build();
    assert_eq!(d.smart_temperature_c, Some(42.5));
    assert_eq!(d.smart_critical_warning, Some(false));
    assert_eq!(d.smart_temp_critical_c, Some(85.0));
    assert_eq!(d.smart_percent_used, Some(15.0));
    assert_eq!(d.smart_power_on_hours, Some(8760));
    assert_eq!(d.name, "nvme0n1");
}

#[test]
fn test_smart_critical_warning_round_trips_three_states() {
    // The UI keys off `smart_critical_warning == Some(true)` (smart_dialog /
    // alerts), so the wire must preserve Some(true) / Some(false) distinctly.
    // The old assertion compared `Some(true) != Some(false)` — that exercises
    // `Option<bool>`'s derived PartialEq, not the DiskMetrics wire contract.
    // (The None baseline is covered by test_disk_metrics_default_smart_fields_none.)
    for value in [true, false] {
        let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .smart_critical_warning(Some(value))
            .build();
        let wire = serde_json::to_value(&disk).expect("DiskMetrics serializes");
        let back: DiskMetrics = serde_json::from_value(wire).expect("DiskMetrics deserializes");
        assert_eq!(back.smart_critical_warning, Some(value));
    }
}

#[test]
fn test_smart_percent_used_round_trips() {
    // Provider-normalized endurance can exceed 100 (over-provisioned wear), so
    // the f32 must round-trip across the whole realistic range. The old test
    // asserted `100.0 >= 100.0` on hardcoded values — trivially true for any
    // f64 and oblivious to the DiskMetrics wire contract.
    for value in [0.0_f32, 15.0, 50.0, 99.5, 100.0, 120.0] {
        let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .smart_percent_used(Some(value))
            .build();
        let wire = serde_json::to_value(&disk).expect("DiskMetrics serializes");
        let back: DiskMetrics = serde_json::from_value(wire).expect("DiskMetrics deserializes");
        assert!(
            (back.smart_percent_used.unwrap() - value).abs() < f32::EPSILON,
            "smart_percent_used {value} did not round-trip"
        );
    }
}

#[test]
fn test_disk_metrics_default_smart_fields_none() {
    let d = DiskMetrics::default();
    assert_eq!(d.connection(), StorageConnection::default());
    assert_eq!(d.smart_availability, SmartAvailability::Unavailable);
    assert_eq!(d.smart_temperature_c, None);
    assert_eq!(d.smart_critical_warning, None);
    assert_eq!(d.smart_temp_critical_c, None);
    assert_eq!(d.smart_percent_used, None);
    assert_eq!(d.smart_power_on_hours, None);
}

#[test]
fn schema_v1_storage_tokens_hydrate_typed_connection_and_round_trip() {
    for (token, expected) in [
        (
            "nvme",
            StorageConnection::new(
                StorageProtocol::Nvme,
                StorageInterconnect::Pcie,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "sata",
            StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Sata,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "sas",
            StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Sas,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "scsi",
            StorageConnection::new(
                StorageProtocol::Scsi,
                StorageInterconnect::Unknown,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "usb",
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Usb,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "virtio",
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Virtio,
                StorageDeviceKind::Virtual,
            ),
        ),
        (
            "mmc",
            StorageConnection::new(
                StorageProtocol::Mmc,
                StorageInterconnect::Mmc,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "ufs",
            StorageConnection::new(
                StorageProtocol::Ufs,
                StorageInterconnect::Ufs,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "ide",
            StorageConnection::new(
                StorageProtocol::Ata,
                StorageInterconnect::Ide,
                StorageDeviceKind::Physical,
            ),
        ),
        (
            "device_mapper",
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Platform,
                StorageDeviceKind::Virtual,
            ),
        ),
        (
            "software_raid",
            StorageConnection::new(
                StorageProtocol::Unknown,
                StorageInterconnect::Platform,
                StorageDeviceKind::Aggregate,
            ),
        ),
    ] {
        let disk: DiskMetrics = serde_json::from_value(serde_json::json!({
            "device_id": "disk:legacy-token",
            "name": "legacy-token",
            "transport": token,
        }))
        .expect("schema-v1 storage token remains readable");
        assert_eq!(disk.connection(), expected);
        let value = serde_json::to_value(&disk).expect("canonical disk serializes");
        assert_eq!(
            value.get("transport").and_then(|value| value.as_str()),
            Some(token)
        );
        let round_trip: DiskMetrics = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.connection(), expected);
    }
}

#[test]
fn test_legacy_disk_transport_derives_new_connection_axes() {
    use taskmanager::core::metrics::{StorageInterconnect, StorageProtocol};

    let disk: DiskMetrics = serde_json::from_value(serde_json::json!({
        "name": "legacy-usb",
        "disk_type": "USB Storage",
        "transport": "usb",
        "model": "",
        "mount_point": "",
        "fs_type": "",
        "total_bytes": 0,
        "available_bytes": 0,
        "read_bytes_per_sec": 0,
        "write_bytes_per_sec": 0,
        "iops": 0,
        "active_time_pct": 0.0,
        "response_time_ms": 0.0,
        "removable": false
    }))
    .expect("legacy disk snapshot");

    let connection = disk.connection();
    assert_eq!(connection.interconnect, StorageInterconnect::Usb);
    assert_eq!(connection.protocol, StorageProtocol::Unknown);
    assert_eq!(disk.media_removable(), None);
}

#[test]
fn test_disk_smart_availability_serializes_as_stable_snake_case() {
    for (availability, expected) in [
        (SmartAvailability::Available, "available"),
        (SmartAvailability::Unsupported, "unsupported"),
        (SmartAvailability::Unavailable, "unavailable"),
        (SmartAvailability::MissingTool, "missing_tool"),
        (SmartAvailability::PermissionDenied, "permission_denied"),
    ] {
        let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
            .smart_availability(availability)
            .build();
        let value = serde_json::to_value(&disk).unwrap();
        assert_eq!(value["smart_availability"], expected);
        let round_trip: DiskMetrics = serde_json::from_value(value).unwrap();
        assert_eq!(round_trip.smart_availability, availability);
    }
}

#[test]
fn test_smart_availability_ui_keys_are_exhaustive_and_distinct() {
    use taskmanager_gpui::gpui_app::perf_views::smart_availability_i18n_key;

    let keys = [
        smart_availability_i18n_key(SmartAvailability::Available),
        smart_availability_i18n_key(SmartAvailability::Unsupported),
        smart_availability_i18n_key(SmartAvailability::Unavailable),
        smart_availability_i18n_key(SmartAvailability::MissingTool),
        smart_availability_i18n_key(SmartAvailability::PermissionDenied),
    ];
    for (index, key) in keys.iter().enumerate() {
        assert!(keys[..index].iter().all(|prior| prior != key));
        assert_ne!(taskmanager::i18n::t(key), *key);
    }
}

#[test]
fn test_smart_status_alone_counts_as_reported_ui_data() {
    use taskmanager::core::device_state::DeviceStatus;
    use taskmanager_gpui::gpui_app::perf_views::{effective_smart_status, has_smart_fields};

    let status_only = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .smart_availability(SmartAvailability::Available)
        .smart_critical_warning(Some(false))
        .build();
    assert!(has_smart_fields(&status_only));
    assert!(!has_smart_fields(&DiskMetrics::default()));

    let legacy_missing_tool = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .smart_availability(SmartAvailability::MissingTool)
        .build();
    assert_eq!(
        effective_smart_status(&legacy_missing_tool),
        DeviceStatus::MissingTool
    );
}
