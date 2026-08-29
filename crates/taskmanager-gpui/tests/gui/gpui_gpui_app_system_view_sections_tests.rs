use super::*;
use taskmanager_core::core::metrics::SystemSnapshot;

fn hardware() -> HardwareInfo {
    HardwareInfo {
        os_name: Some("ExampleOS".into()),
        os_version: Some("1.0".into()),
        kernel_version: Some("6.1.0-example".into()),
        hostname: Some("lab-machine".into()),
        cpu_brand: Some("Example CPU 8".into()),
        ..Default::default()
    }
}

/// The newly surfaced session facts appear only when the adapter reported
/// them — a server/Windows host omits every one of the rows.
#[test]
fn device_section_surfaces_session_facts_only_when_present() {
    let bare = device_section(&hardware());
    let labels = |s: &SystemSection| s.rows.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
    let bare_labels = labels(&bare);
    for key in [
        "system.product_version",
        "system.package_manager",
        "system.desktop_environment",
        "system.windowing_system",
    ] {
        assert!(
            !bare_labels.iter().any(|label| label == i18n::t(key)),
            "{key} row must be omitted when absent"
        );
    }

    let rich = device_section(&HardwareInfo {
        product_version: Some("v2".into()),
        package_manager: Some("apt".into()),
        package_manager_version: Some("2.6".into()),
        package_count: Some(1489),
        desktop_environment_version: Some("KDE Plasma 6.1".into()),
        windowing_system: Some("Wayland".into()),
        ..hardware()
    });
    let rich_labels = labels(&rich);
    assert!(rich_labels.contains(&i18n::t("system.product_version").to_string()));
    assert!(rich_labels.contains(&i18n::t("system.desktop_environment").to_string()));
    let package_row = rich
        .rows
        .iter()
        .find(|(k, _)| k == i18n::t("system.package_manager"))
        .expect("package manager row");
    assert_eq!(package_row.1, "apt 2.6", "version joins the manager label");
    let package_count_row = rich
        .rows
        .iter()
        .find(|(k, _)| k == i18n::t("system.package_count"))
        .expect("package count row");
    assert_eq!(package_count_row.1, "1489");
}

#[test]
fn device_section_surfaces_edid_display_facts_as_one_compact_row() {
    let rich = HardwareInfo {
        displays: vec![taskmanager_core::core::hardware::DisplayInfo {
            connector: "DP-1".into(),
            manufacturer: Some("DEL".into()),
            model: Some("TaskPanel".into()),
            width_px: Some(1920),
            height_px: Some(1080),
            refresh_hz: Some(60.0),
            hdr_supported: Some(true),
            ..Default::default()
        }],
        ..hardware()
    };
    let row = device_section(&rich)
        .rows
        .into_iter()
        .find(|(label, _)| label == i18n::t("system.display"))
        .expect("display row");
    assert!(
        row.1
            .starts_with("DP-1 · DEL TaskPanel · 1920×1080 · 60.0 Hz")
    );
    assert!(row.1.contains(i18n::t("system.hdr")));
}

/// Kernel boot facts stay adjacent to the Kernel row and disappear off
/// the non-Linux providers (platform-difference honesty).
#[test]
fn device_section_kernel_facts_stay_conditional() {
    let mut hw = hardware();
    hw.kernel_modules_count = Some(42);
    hw.kernel_cmdline = Some("quiet splash".into());
    let rows = device_section(&hw).rows;
    let kernel_ix = rows
        .iter()
        .position(|(k, _)| k == i18n::t("system.kernel"))
        .expect("kernel row");
    assert_eq!(rows[kernel_ix + 1].0, i18n::t("system.kernel_modules"));
    assert_eq!(rows[kernel_ix + 1].1, "42");
    assert_eq!(rows[kernel_ix + 2].0, i18n::t("system.boot_args"));

    let bare = device_section(&hardware()).rows;
    assert!(
        !bare
            .iter()
            .any(|(k, _)| k == i18n::t("system.kernel_modules"))
    );
}

/// Homogeneous parts get no P/E/LP rows; a hybrid part gets one row per
/// non-zero class, and the instruction set becomes chips.
#[test]
fn cpu_section_breaks_down_hybrid_topology_and_feature_chips() {
    use taskmanager_core::core::hardware::CoreBreakdown;
    let mut hw = hardware();
    hw.instruction_features = vec![
        taskmanager_core::core::CpuInstructionFeature::AesNi,
        taskmanager_core::core::CpuInstructionFeature::Avx2,
    ];
    let homogeneous = cpu_section(&hw, &SystemSnapshot::default());
    let p_label = i18n::t("cpu.performance_cores").to_string();
    let e_label = i18n::t("cpu.efficiency_cores").to_string();
    assert!(
        !homogeneous
            .rows
            .iter()
            .any(|(k, _)| *k == p_label || *k == e_label),
        "homogeneous part renders no hybrid rows"
    );
    assert_eq!(homogeneous.chips.len(), 2, "features become chips");

    hw.core_breakdown = CoreBreakdown {
        p_cores: 4,
        e_cores: 8,
        lp_cores: 0,
    };
    let hybrid = cpu_section(&hw, &SystemSnapshot::default());
    assert!(
        hybrid.rows.iter().any(|(k, _)| *k == p_label),
        "hybrid part renders the P-core row"
    );
    assert!(hybrid.rows.iter().any(|(k, _)| *k == e_label));
}

/// The System memory card is static: installed capacity and module facts are
/// shown, while live used/available values never create a meter here.
#[test]
fn memory_section_uses_static_capacity_and_conditional_rows() {
    let hardware = HardwareInfo {
        total_memory_mb: Some(32 * 1024),
        ..hardware()
    };
    let snap = SystemSnapshot::default();
    let s = memory_section(&hardware, &snap);
    assert!(
        s.meters.is_empty(),
        "live memory usage must stay off System"
    );
    let capacity = s
        .rows
        .iter()
        .find(|(k, _)| k == i18n::t("common.memory"))
        .expect("installed capacity row");
    assert_eq!(capacity.1, "32768 MiB");
    assert!(
        !s.rows.iter().any(|(k, _)| k == i18n::t("mem.swap")),
        "no swap configured → no swap row"
    );

    // Missing hardware inventory stays an honest dash even if telemetry has a
    // live total; the page must not fall back to a changing runtime scalar.
    let s = memory_section(&HardwareInfo::default(), &snap);
    let capacity = s
        .rows
        .iter()
        .find(|(k, _)| k == i18n::t("common.memory"))
        .expect("capacity row still renders");
    assert_eq!(capacity.1, taskmanager_shell::presentation::MISSING_VALUE);
}

/// GPU identity/capacity remains stable when live utilization, temperature, or
/// driver fields change.
#[test]
fn graphics_section_omits_live_gpu_facts() {
    use taskmanager_core::core::metrics::GpuMetrics;
    let mut gpu = GpuMetrics::new("", "Example Arc");
    let bare = graphics_section(
        &SystemSnapshot {
            gpu: vec![gpu.clone()],
            ..SystemSnapshot::default()
        },
        None,
    );
    assert_eq!(bare.rows[0].1, "Example Arc");

    gpu.apply_scalar_observations(taskmanager_core::core::metrics::GpuScalarObservations {
        utilization_pct: taskmanager_core::core::metrics::ScalarObservation::available(37.5, 1),
        temperature_c: taskmanager_core::core::metrics::ScalarObservation::available(61.0, 1),
        ..Default::default()
    });
    gpu.driver = Some("xe".into());
    let live = graphics_section(
        &SystemSnapshot {
            gpu: vec![gpu],
            ..SystemSnapshot::default()
        },
        None,
    );
    assert_eq!(live.rows[0].1, "Example Arc");
}

#[test]
fn mc04_npu_current_case_graphics_section_projects_complete_current_npu_facts_without_fabricating_gaps()
 {
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::{
        DeviceId, FailureKind, NpuDevice, NpuEngineKind, NpuEngineUsage, NpuInventorySnapshot,
        NpuMemoryReport,
    };

    let inventory = NpuInventorySnapshot::discovered(
        vec![NpuDevice {
            device_id: DeviceId::new("accel:npu0"),
            brand: Some("Example Neural Engine".into()),
            driver: Some("example_npu".into()),
            utilization_pct: ScalarObservation::available(37.5, 10),
            engines: vec![
                NpuEngineUsage {
                    kind: NpuEngineKind::Matrix,
                    utilization_pct: ScalarObservation::available(12.5, 10),
                },
                NpuEngineUsage {
                    kind: NpuEngineKind::Copy,
                    utilization_pct: ScalarObservation::unavailable(FailureKind::Unsupported),
                },
            ],
            memory: NpuMemoryReport {
                dedicated_total_bytes: ScalarObservation::available(0, 10),
                shared_total_bytes: ScalarObservation::available(1024 * 1024 * 1024, 10),
            },
            ..Default::default()
        }],
        10,
    );
    let section = graphics_section(&SystemSnapshot::default(), Some(&inventory));
    assert_eq!(section.rows[0].1, "Example Neural Engine (example_npu)");
    assert_eq!(section.rows[1].1, "38%");
    assert_eq!(section.rows[2].1, "13%");
    assert_eq!(
        section.rows[3].1,
        taskmanager_shell::presentation::MISSING_VALUE,
        "a reported engine with unavailable utilization stays visible as a gap"
    );
    assert_eq!(
        section.rows[4].1,
        crate::gpui_app::formatting::bytes_to_human(0),
        "measured zero dedicated memory must not be confused with unavailable"
    );
    assert_eq!(
        section.rows[5].1,
        crate::gpui_app::formatting::bytes_to_human(1024 * 1024 * 1024)
    );

    let failed = NpuInventorySnapshot::failed(FailureKind::Unsupported, "no provider", 11);
    assert!(
        graphics_section(&SystemSnapshot::default(), Some(&failed)).is_empty(),
        "a failed inventory cannot fabricate an NPU identity or telemetry row"
    );
}

#[test]
fn storage_section_surfaces_identity_type_and_capacity_without_live_io() {
    let disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .name("nvme0n1".into())
        .model("Example NVMe".into())
        .disk_type("NVMe SSD".into())
        .current_capacity_bytes(2 * 1024 * 1024 * 1024 * 1024)
        .current_read_bytes_per_sec(99)
        .current_write_bytes_per_sec(88)
        .build();
    let section = storage_section(&SystemSnapshot {
        disks: vec![disk],
        ..SystemSnapshot::default()
    });
    assert_eq!(section.rows[0].0, i18n::t("common.disk"));
    assert_eq!(section.rows[0].1, "Example NVMe · NVMe SSD · 2048.0 GiB");
}

/// Sections with zero facts drop out of the page entirely.
#[test]
fn empty_sections_are_omitted() {
    let sections = build_sections(&hardware(), &SystemSnapshot::default(), None);
    // Default host: graphics has no devices and the static inventory sections
    // still never emit empty cards.
    assert!(
        !sections
            .iter()
            .any(|s| s.title_key == "system.section.graphics"),
        "no GPUs/NPUs → no graphics section"
    );
    assert!(sections.iter().all(|s| !s.is_empty()));
}

/// Tiles are static hardware parameters and do not mirror live CPU/memory or
/// process/uptime values.
#[test]
fn tiles_show_static_hardware_parameters() {
    let hardware = HardwareInfo {
        total_memory_mb: Some(16 * 1024),
        ..hardware()
    };
    let tiles = build_tiles(&hardware, &SystemSnapshot::default());
    assert_eq!(tiles.len(), 4);
    assert_eq!(tiles[0].value, "Example CPU 8");
    assert_eq!(tiles[1].value, "16384 MiB");
    assert_eq!(
        tiles[2].value,
        taskmanager_shell::presentation::MISSING_VALUE
    );
    assert_eq!(
        tiles[3].value,
        taskmanager_shell::presentation::MISSING_VALUE
    );
}
