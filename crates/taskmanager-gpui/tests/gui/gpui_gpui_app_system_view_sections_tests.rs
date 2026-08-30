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
    let bare = device_section(&hardware(), &SmbiosMemoryState::Closed);
    let labels = |s: &SystemSection| s.rows.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>();
    let bare_labels = labels(&bare);
    for key in [
        "system.product_version",
        "system.package_manager",
        "system.desktop_environment",
        "system.windowing_system",
        "system.field.chipset",
    ] {
        assert!(
            !bare_labels.iter().any(|label| label == i18n::t(key)),
            "{key} row must be omitted when absent"
        );
    }

    let rich = device_section(
        &HardwareInfo {
            product_version: Some("v2".into()),
            package_manager: Some("apt".into()),
            package_manager_version: Some("2.6".into()),
            package_count: Some(1489),
            desktop_environment_version: Some("KDE Plasma 6.1".into()),
            windowing_system: Some("Wayland".into()),
            chipset: Some("Z690 Chipset".into()),
            ..hardware()
        },
        &SmbiosMemoryState::Closed,
    );
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
    let chipset_row = rich
        .rows
        .iter()
        .find(|(k, _)| k == i18n::t("system.field.chipset"))
        .expect("chipset row when the adapter proved one");
    assert_eq!(chipset_row.1, "Z690 Chipset");
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
    let row = device_section(&rich, &SmbiosMemoryState::Closed)
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
    let rows = device_section(&hw, &SmbiosMemoryState::Closed).rows;
    let kernel_ix = rows
        .iter()
        .position(|(k, _)| k == i18n::t("system.kernel"))
        .expect("kernel row");
    assert_eq!(rows[kernel_ix + 1].0, i18n::t("system.kernel_modules"));
    assert_eq!(rows[kernel_ix + 1].1, "42");
    assert_eq!(rows[kernel_ix + 2].0, i18n::t("system.boot_args"));

    let bare = device_section(&hardware(), &SmbiosMemoryState::Closed).rows;
    assert!(
        !bare
            .iter()
            .any(|(k, _)| k == i18n::t("system.kernel_modules"))
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// SMBIOS identity rows (the root-only DMI facts from the same request lane)
// ─────────────────────────────────────────────────────────────────────────────

/// Drive the real session through admission so the projection observes the
/// same Ready state production produces.
fn ready_state(identity: Option<taskmanager_core::DmiIdentityFacts>) -> SmbiosMemoryState {
    use taskmanager_application::SmbiosMemorySession;
    use taskmanager_platform_contract::RequestId;
    let mut session = SmbiosMemorySession::default();
    let attempt = session.begin_attempt();
    let request = RequestId::new(41).expect("fixture request id");
    assert!(session.accept_attempt(attempt, request));
    assert!(session.complete(
        request,
        taskmanager_core::SmbiosMemorySnapshot::success(2, 1, Vec::new(), identity),
    ));
    session.state().clone()
}

fn identity_facts() -> taskmanager_core::DmiIdentityFacts {
    taskmanager_core::DmiIdentityFacts {
        bios_vendor: Some("AMI".into()),
        bios_version: Some("P1.27".into()),
        bios_date: Some("04/17/2024".into()),
        board_manufacturer: Some("ASUSTeK".into()),
        board_product: Some("X670E".into()),
        board_serial: Some("MB-SN-1".into()),
        board_asset_tag: Some("ASSET-42".into()),
        system_manufacturer: Some("LENOVO".into()),
        system_product: Some("21JX".into()),
        system_serial: Some("PF3XYZ42".into()),
        system_uuid: Some("4c4c4544-0042-3510-8054-b7c04f4d3532".into()),
        system_sku: Some("SKU-AB".into()),
        system_family: Some("ThinkPad".into()),
    }
}

/// A Ready lane with identity facts renders the serial/UUID/asset-tag/SKU
/// rows with exactly the accepted values — never a dash, never a fabrication.
#[test]
fn device_section_renders_smbios_identity_rows_from_a_ready_lane() {
    let section = device_section(&hardware(), &ready_state(Some(identity_facts())));
    let row = |key: &'static str| {
        section
            .rows
            .iter()
            .find(|(label, _)| label == i18n::t(key))
            .unwrap_or_else(|| panic!("{key} row must render"))
            .1
            .clone()
    };
    assert_eq!(row("system.field.system_serial"), "PF3XYZ42");
    assert_eq!(
        row("system.field.product_uuid"),
        "4c4c4544-0042-3510-8054-b7c04f4d3532"
    );
    assert_eq!(row("system.field.asset_tag"), "ASSET-42");
    assert_eq!(row("system.field.system_sku"), "SKU-AB");
}

/// A fact the record did not state omits its row; a lane without an accepted
/// payload (Closed) renders no identity rows at all.
#[test]
fn device_section_identity_rows_stay_honest_when_facts_are_absent() {
    let mut facts = identity_facts();
    facts.system_uuid = None;
    facts.system_sku = None;
    let partial = device_section(&hardware(), &ready_state(Some(facts)));
    let labels = partial
        .rows
        .iter()
        .map(|(label, _)| label.clone())
        .collect::<Vec<_>>();
    assert!(
        !labels.contains(&i18n::t("system.field.product_uuid").to_string()),
        "an absent UUID omits its row"
    );
    assert!(
        !labels.contains(&i18n::t("system.field.system_sku").to_string()),
        "an absent SKU omits its row"
    );
    assert!(labels.contains(&i18n::t("system.field.system_serial").to_string()));

    // No identity tables on the host: every identity row disappears.
    let none = device_section(&hardware(), &ready_state(None));
    assert!(
        !none
            .rows
            .iter()
            .any(|(label, _)| label == i18n::t("system.field.system_serial"))
    );

    // No accepted payload: nothing renders.
    let closed = device_section(&hardware(), &SmbiosMemoryState::Closed);
    assert!(
        !closed
            .rows
            .iter()
            .any(|(label, _)| label == i18n::t("system.field.asset_tag"))
    );
}

/// A refresh in flight keeps the last accepted identity visible — the same
/// last-good rule the memory subsection applies, so the two regions stay in
/// step during a lane refresh.
#[test]
fn device_section_keeps_last_good_identity_while_refreshing() {
    use taskmanager_application::SmbiosMemorySession;
    let mut session = SmbiosMemorySession::default();
    let attempt = session.begin_attempt();
    let request = taskmanager_platform_contract::RequestId::new(7).expect("fixture id");
    assert!(session.accept_attempt(attempt, request));
    assert!(session.complete(
        request,
        taskmanager_core::SmbiosMemorySnapshot::success(2, 1, Vec::new(), Some(identity_facts())),
    ));
    let _ = session.begin_attempt();
    let section = device_section(&hardware(), session.state());
    assert!(
        section.rows.iter().any(
            |(label, value)| label == i18n::t("system.field.system_serial") && value == "PF3XYZ42"
        ),
        "the last accepted identity stays visible during a refresh"
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
    let homogeneous = cpu_section(
        &hw,
        &SystemSnapshot::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
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
    let hybrid = cpu_section(
        &hw,
        &SystemSnapshot::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    assert!(
        hybrid.rows.iter().any(|(k, _)| *k == p_label),
        "hybrid part renders the P-core row"
    );
    assert!(hybrid.rows.iter().any(|(k, _)| *k == e_label));
}

/// The CPU section mirrors the details panel's identity projection: rows
/// appear only when the CPUID identity was actually probed.
#[test]
fn cpu_section_surfaces_cpuid_identity_only_when_probed() {
    use taskmanager_core::core::hardware::CpuIdentity;
    let bare = cpu_section(
        &hardware(),
        &SystemSnapshot::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    let vendor_label = i18n::t("system.cpu_vendor");
    assert!(
        !bare.rows.iter().any(|(k, _)| *k == vendor_label),
        "unprobed identity must render no vendor row"
    );

    let mut probed_hw = hardware();
    probed_hw.cpu_identity =
        CpuIdentity::from_cpuid_parts(Some("AuthenticAMD".into()), 0xF, 0xA, 0x1, 0x2, 0x0);
    let probed = cpu_section(
        &probed_hw,
        &SystemSnapshot::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    let vendor_row = probed
        .rows
        .iter()
        .find(|(k, _)| *k == vendor_label)
        .expect("probed identity must render the vendor row");
    assert_eq!(vendor_row.1, "AuthenticAMD");
    let identity_row = probed
        .rows
        .iter()
        .find(|(k, _)| *k == i18n::t("system.cpu_identity"))
        .expect("probed identity must render the family/model/stepping row");
    assert_eq!(identity_row.1, "25 / 33 / 0");
    let codename_row = probed
        .rows
        .iter()
        .find(|(k, _)| *k == i18n::t("system.cpu_codename"))
        .expect("the Vermeer pair must resolve through the shared table");
    assert_eq!(codename_row.1, "Zen 3 (Vermeer)");
    let process_row = probed
        .rows
        .iter()
        .find(|(k, _)| *k == i18n::t("system.cpu_process"))
        .expect("the Vermeer pair must resolve its process node");
    assert_eq!(process_row.1, "TSMC N7");
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
    let s = memory_section(
        &hardware,
        &snap,
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    assert!(
        s.meters.is_empty(),
        "live memory usage must stay off System"
    );
    let capacity = s
        .rows
        .iter()
        .find(|(k, _)| k == i18n::t("common.memory"))
        .expect("installed capacity row");
    assert_eq!(capacity.1, "32.0 GiB");
    assert!(
        !s.rows.iter().any(|(k, _)| k == i18n::t("mem.swap")),
        "no swap configured → no swap row"
    );

    // Missing hardware inventory stays an honest dash even if telemetry has a
    // live total; the page must not fall back to a changing runtime scalar.
    let s = memory_section(
        &HardwareInfo::default(),
        &snap,
        taskmanager_core::core::units::UnitPreferences::default(),
    );
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
        taskmanager_core::core::units::UnitPreferences::default(),
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
        taskmanager_core::core::units::UnitPreferences::default(),
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
    let section = graphics_section(
        &SystemSnapshot::default(),
        Some(&inventory),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
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
        taskmanager_core::core::units::UnitPreferences::default().format_quantity(
            0,
            taskmanager_core::core::units::QuantityFamily::Memory,
            false
        ),
        "measured zero dedicated memory must not be confused with unavailable"
    );
    assert_eq!(
        section.rows[5].1,
        taskmanager_core::core::units::UnitPreferences::default().format_quantity(
            1024 * 1024 * 1024,
            taskmanager_core::core::units::QuantityFamily::Memory,
            false
        )
    );

    let failed = NpuInventorySnapshot::failed(FailureKind::Unsupported, "no provider", 11);
    assert!(
        graphics_section(
            &SystemSnapshot::default(),
            Some(&failed),
            taskmanager_core::core::units::UnitPreferences::default()
        )
        .is_empty(),
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
    let section = storage_section(
        &SystemSnapshot {
            disks: vec![disk],
            ..SystemSnapshot::default()
        },
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    assert_eq!(section.rows[0].0, i18n::t("common.disk"));
    assert_eq!(section.rows[0].1, "Example NVMe · NVMe SSD · 2048.0 GiB");
}

/// Sections with zero facts drop out of the page entirely.
#[test]
fn empty_sections_are_omitted() {
    let sections = build_sections(
        &hardware(),
        &SystemSnapshot::default(),
        None,
        &SmbiosMemoryState::Closed,
        taskmanager_core::core::units::UnitPreferences::default(),
    );
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
    let tiles = build_tiles(
        &hardware,
        &SystemSnapshot::default(),
        taskmanager_core::core::units::UnitPreferences::default(),
    );
    assert_eq!(tiles.len(), 4);
    assert_eq!(tiles[0].value, "Example CPU 8");
    assert_eq!(tiles[1].value, "16.0 GiB");
    assert_eq!(
        tiles[2].value,
        taskmanager_shell::presentation::MISSING_VALUE
    );
    assert_eq!(
        tiles[3].value,
        taskmanager_shell::presentation::MISSING_VALUE
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// SMBIOS memory-inventory subsection (the escalation-backed request lane)
// ─────────────────────────────────────────────────────────────────────────────

mod memory_inventory_lane {
    use super::super::memory_inventory::{
        MemoryInventoryInputs, MemoryInventoryModel, memory_inventory_model,
    };
    use taskmanager_application::SmbiosMemorySession;
    use taskmanager_application::i18n;
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::metrics::{SmbiosMemorySnapshot, SmbiosModuleRow};
    use taskmanager_core::core::units::UnitPreferences;
    use taskmanager_platform_contract::{CapabilityStatus, RequestId};

    fn inputs<'a>(
        state: &'a taskmanager_application::SmbiosMemoryState,
        capability: Option<CapabilityStatus>,
    ) -> MemoryInventoryInputs<'a> {
        MemoryInventoryInputs { state, capability }
    }

    fn model(
        state: &taskmanager_application::SmbiosMemoryState,
        capability: Option<CapabilityStatus>,
    ) -> MemoryInventoryModel {
        memory_inventory_model(&inputs(state, capability), UnitPreferences::default())
    }

    /// Drive the real session through the submission path so the projections
    /// observe the same states production admission produces.
    fn accept_snapshot(
        session: &mut SmbiosMemorySession,
        request: u64,
        snapshot: SmbiosMemorySnapshot,
    ) {
        let attempt = session.begin_attempt();
        let request_id = RequestId::new(request).expect("fixture request id");
        assert!(session.accept_attempt(attempt, request_id));
        assert!(session.complete(request_id, snapshot));
    }

    fn module(slot: u32, locator: Option<&str>, part: Option<&str>) -> SmbiosModuleRow {
        SmbiosModuleRow {
            slot,
            size_mb: Some(16 * 1024),
            speed_mts: Some(5600),
            configured_speed_mts: Some(5200),
            manufacturer: Some("ExampleWorks".into()),
            serial_number: Some("SERIAL1".into()),
            part_number: part.map(str::to_string),
            form_factor: Some("SODIMM".into()),
            memory_type: Some("DDR5".into()),
            locator: locator.map(str::to_string),
        }
    }

    /// Ready renders the slots row plus one row per populated module with
    /// exactly the facts the SMBIOS record carried.
    #[test]
    fn ready_inventory_projects_slots_and_module_rows() {
        let mut session = SmbiosMemorySession::default();
        accept_snapshot(
            &mut session,
            1,
            SmbiosMemorySnapshot::success(
                4,
                2,
                vec![
                    module(0, Some("ChannelA-DIMM0"), Some("PART-A")),
                    SmbiosModuleRow {
                        slot: 2,
                        ..SmbiosModuleRow::default()
                    },
                ],
                // The identity facts are a separate projection lane; the
                // memory rows must not depend on them.
                None,
            ),
        );
        match model(session.state(), Some(CapabilityStatus::Available)) {
            MemoryInventoryModel::Inventory(rows) => {
                assert_eq!(rows[0].0, i18n::t("system.memory_slots"));
                assert_eq!(
                    rows[0].1,
                    format!("2 / 4 {}", i18n::t("common.used")),
                    "slots used/total leads the inventory"
                );
                assert_eq!(rows[1].0, "ChannelA-DIMM0");
                assert_eq!(rows[1].1, "PART-A · 16.0 GiB · 5200 MT/s");
                // No locator: the slot-indexed label; a record with no
                // readable facts keeps its row with the shared dash.
                assert_eq!(rows[2].0, format!("{} 2", i18n::t("system.memory_module")));
                assert_eq!(rows[2].1, taskmanager_shell::presentation::MISSING_VALUE);
            }
            other => panic!("accepted payload must project rows, got {other:?}"),
        }
    }

    /// RequiresEscalation is the affordance state: no slot or module row may
    /// render while the capability gap is the visible fact.
    #[test]
    fn requires_escalation_projects_the_affordance_not_rows() {
        let mut session = SmbiosMemorySession::default();
        accept_snapshot(
            &mut session,
            1,
            SmbiosMemorySnapshot::failed(FailureKind::RequiresEscalation, "fixture"),
        );
        let projected = model(session.state(), Some(CapabilityStatus::Available));
        assert_eq!(projected, MemoryInventoryModel::AuthorizationRequired);
        assert!(
            !matches!(projected, MemoryInventoryModel::Inventory(_)),
            "an escalation gap must never fabricate a slot or module row"
        );
    }

    /// Other failure kinds keep their typed labels; none of them is a number.
    #[test]
    fn other_failures_project_typed_unavailable_labels() {
        for (kind, key) in [
            (
                FailureKind::PermissionDenied,
                "system.memory_inventory_denied",
            ),
            (
                FailureKind::MissingDependency,
                "system.memory_inventory_helper",
            ),
            (
                FailureKind::Unsupported,
                "system.memory_inventory_unsupported",
            ),
            (FailureKind::TimedOut, "system.memory_inventory_unavailable"),
            (
                FailureKind::TemporarilyUnavailable,
                "system.memory_inventory_unavailable",
            ),
            (
                FailureKind::ProviderFault,
                "system.memory_inventory_unavailable",
            ),
        ] {
            let mut session = SmbiosMemorySession::default();
            accept_snapshot(
                &mut session,
                1,
                SmbiosMemorySnapshot::failed(kind, "fixture detail"),
            );
            assert_eq!(
                model(session.state(), Some(CapabilityStatus::Available)),
                MemoryInventoryModel::Unavailable(key),
                "failure kind {kind:?}"
            );
        }
    }

    /// Closed renders nothing while no lane exists; a registered escalation
    /// lane is the single authorize entry. A first request with no accepted
    /// payload is the pending row, and a refresh keeps the last rows.
    #[test]
    fn closed_and_loading_states_render_honestly() {
        let closed = taskmanager_application::SmbiosMemoryState::Closed;
        assert_eq!(
            model(&closed, None),
            MemoryInventoryModel::Hidden,
            "no registered capability → no subsection at all"
        );
        assert_eq!(
            model(&closed, Some(CapabilityStatus::Unsupported)),
            MemoryInventoryModel::Hidden
        );
        assert_eq!(
            model(&closed, Some(CapabilityStatus::Available)),
            MemoryInventoryModel::AuthorizationRequired,
            "a registered lane offers the explicit authorize entry"
        );

        let mut session = SmbiosMemorySession::default();
        accept_snapshot(
            &mut session,
            1,
            SmbiosMemorySnapshot::success(2, 1, vec![module(0, Some("DIMM0"), Some("PART"))], None),
        );
        let _ = session.begin_attempt();
        assert!(matches!(
            model(session.state(), Some(CapabilityStatus::Available)),
            MemoryInventoryModel::Inventory(rows) if rows.len() == 2
        ));
        let mut fresh = SmbiosMemorySession::default();
        let _ = fresh.begin_attempt();
        assert_eq!(
            model(fresh.state(), Some(CapabilityStatus::Available)),
            MemoryInventoryModel::Reading
        );
    }
}
