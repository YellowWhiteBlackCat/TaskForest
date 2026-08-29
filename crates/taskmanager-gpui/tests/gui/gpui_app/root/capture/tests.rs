#[path = "tests/dashboard.rs"]
mod dashboard;
#[path = "tests/live.rs"]
mod live;

use super::{
    CaptureEvidence, CaptureProcessAction, CaptureScenario, DashboardState, ProcessBatchAction,
    ProcessDetailsSection, ProcessInsightsState, ProcessItem, ProcessTerminationAction, ServiceId,
    SystemHealthCaptureOutcome, SystemSection, SystemSnapshot, TopPage,
};
use taskmanager_core::core::process::{ProcessApplicationIdentity, ProcessMetadataObservation};
use taskmanager_core::core::startup::{StartupImpactEvidence, StartupImpactUnknownReason};
use taskmanager_core::core::{ScalarObservation, SmartAvailability};

impl CaptureEvidence {
    pub(super) fn for_test(scenario: Option<CaptureScenario>) -> Self {
        Self {
            enabled: true,
            scenario,
            ..Self::default()
        }
    }
}

#[test]
fn scenario_tokens_parse_strictly() {
    assert_eq!(
        CaptureScenario::parse("smart-missing-tool"),
        Some(CaptureScenario::SmartMissingTool)
    );
    assert_eq!(
        CaptureScenario::parse("process-force-kill"),
        Some(CaptureScenario::ProcessForceKill)
    );
    assert_eq!(
        CaptureScenario::parse("process-selection"),
        Some(CaptureScenario::ProcessSelection)
    );
    assert_eq!(
        CaptureScenario::parse("process-memory-pss-swap"),
        Some(CaptureScenario::ProcessMemoryPssSwap)
    );
    assert_eq!(
        CaptureScenario::parse("history-60m"),
        Some(CaptureScenario::HistorySixtyMinutes)
    );
    for (token, scenario) in [
        (
            "process-batch-confirm",
            CaptureScenario::ProcessBatchConfirm,
        ),
        ("startup-impact", CaptureScenario::StartupImpact),
        (
            "startup-failure-evidence",
            CaptureScenario::StartupFailureEvidence,
        ),
        ("gpu-engine-inventory", CaptureScenario::GpuEngineInventory),
        ("system-hardware", CaptureScenario::SystemHardware),
        ("system-npu", CaptureScenario::SystemNpu),
        ("diagnostic-preview", CaptureScenario::DiagnosticPreview),
        ("diagnostic-failure", CaptureScenario::DiagnosticFailure),
        (
            "process-network-details",
            CaptureScenario::ProcessNetworkDetails,
        ),
        ("process-gpu-details", CaptureScenario::ProcessGpuDetails),
        (
            "process-resource-limits",
            CaptureScenario::ProcessResourceLimits,
        ),
        ("process-isolation", CaptureScenario::ProcessIsolation),
        ("storage-health", CaptureScenario::StorageHealth),
        (
            "smart-self-test-confirm",
            CaptureScenario::SmartSelfTestConfirm,
        ),
        ("sensor-center", CaptureScenario::SensorCenter),
        (
            "battery-fan-performance",
            CaptureScenario::BatteryFanPerformance,
        ),
        (
            "battery-live-performance",
            CaptureScenario::BatteryLivePerformance,
        ),
        ("partition-disk-usage", CaptureScenario::PartitionDiskUsage),
        ("partition-live-usage", CaptureScenario::PartitionLiveUsage),
        (
            "process-memory-pss-swap",
            CaptureScenario::ProcessMemoryPssSwap,
        ),
        ("keyboard-focus", CaptureScenario::KeyboardFocus),
        (
            "settings-switch-focus",
            CaptureScenario::SettingsSwitchFocus,
        ),
        ("settings-zero-gray", CaptureScenario::SettingsZeroGray),
        (
            "apps-search-highlight",
            CaptureScenario::AppsSearchHighlight,
        ),
        (
            "services-search-highlight",
            CaptureScenario::ServicesSearchHighlight,
        ),
        ("apps-zero-gray", CaptureScenario::AppsZeroGray),
        ("apps-group-expanded", CaptureScenario::AppsGroupExpanded),
        ("apps-identity-matrix", CaptureScenario::AppsIdentityMatrix),
        ("sidebar-hidden", CaptureScenario::SidebarHidden),
        ("sidebar-edit", CaptureScenario::SidebarEdit),
        ("telemetry-paused", CaptureScenario::TelemetryPaused),
        ("system-about", CaptureScenario::SystemAbout),
        ("about", CaptureScenario::About),
        ("first-run", CaptureScenario::FirstRun),
        (
            "application-history-replay",
            CaptureScenario::ApplicationHistoryReplay,
        ),
    ] {
        assert_eq!(CaptureScenario::parse(token), Some(scenario));
        assert_eq!(scenario.token(), token);
    }
    assert_eq!(CaptureScenario::parse(""), None);
    assert_eq!(CaptureScenario::parse("force-kill"), None);
    assert_eq!(CaptureScenario::parse("process-batch-confirmation"), None);
}

#[test]
fn process_selection_capture_targets_the_visible_application_aggregate() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessSelection));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let identity =
        ProcessApplicationIdentity::new("io.example.CaptureTarget", "Capture Target", None)
            .expect("valid capture application identity");
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(42_424)
            .name("capture-host".into())
            .application_identity_observation(ProcessMetadataObservation::available(
                identity.clone(),
                1,
            ))
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(42_425)
            .parent_pid(Some(42_424))
            .name("taskmanager".into())
            .application_identity_observation(ProcessMetadataObservation::available(identity, 1))
            .build(),
    ];

    assert_eq!(
        evidence.on_processes_update(true, &mut processes),
        Some(CaptureProcessAction::ApplicationSelection(42_424))
    );
    assert!(evidence.scenario_ready);
}

#[test]
fn keyboard_capture_waits_for_live_data_and_observed_input_focus() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::KeyboardFocus));
    assert!(!evidence.keyboard_focus_requested());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.keyboard_focus_requested());
    evidence.mark_keyboard_focus_ready();
    assert!(evidence.scenario_ready);
}

#[test]
fn settings_switch_capture_waits_for_data_and_marks_only_after_focus() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SettingsSwitchFocus));
    assert!(!evidence.settings_switch_focus_requested());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.settings_switch_focus_requested());
    evidence.mark_settings_switch_focus_ready();
    assert!(evidence.scenario_ready);
    assert!(!evidence.settings_switch_focus_requested());
}

#[test]
fn settings_zero_gray_capture_waits_for_data_and_marks_only_after_focus() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SettingsZeroGray));
    assert!(!evidence.settings_zero_gray_requested());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.settings_zero_gray_requested());
    evidence.mark_settings_zero_gray_ready();
    assert!(evidence.scenario_ready);
    evidence.mark_settings_zero_gray_ready();
    assert!(evidence.scenario_ready);
    assert!(!evidence.settings_zero_gray_requested());
}

#[test]
fn sidebar_hidden_capture_waits_for_live_data_and_requires_hidden_projection() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SidebarHidden));
    assert!(!evidence.sidebar_hidden_requested());

    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.sidebar_hidden_requested());

    evidence.mark_sidebar_hidden_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_sidebar_hidden_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.sidebar_hidden_requested());
}

#[test]
fn sidebar_edit_capture_waits_for_live_data_and_requires_edit_projection() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SidebarEdit));
    assert!(!evidence.sidebar_edit_requested());

    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.sidebar_edit_requested());

    evidence.mark_sidebar_edit_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_sidebar_edit_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.sidebar_edit_requested());
}

#[test]
fn telemetry_paused_capture_waits_for_live_data_and_requires_paused_projection() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::TelemetryPaused));
    assert!(!evidence.telemetry_paused_requested());

    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.telemetry_paused_requested());

    evidence.mark_telemetry_paused_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_telemetry_paused_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.telemetry_paused_requested());
}

#[test]
fn system_about_capture_waits_for_live_data_and_requires_open_projection() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SystemAbout));
    assert!(!evidence.system_about_requested());

    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.system_about_requested());

    evidence.mark_system_about_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_system_about_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.system_about_requested());
}

#[test]
fn mc07_capture_readiness_case_about_capture_waits_for_live_data_and_requires_open_projection() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::About));
    assert!(!evidence.about_requested());

    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.about_requested());

    evidence.mark_about_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_about_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.about_requested());
}

#[test]
fn first_run_capture_waits_for_live_data_and_uses_fixed_fixture_values() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::FirstRun));
    assert!(!evidence.first_run_requested());

    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = vec![ProcessItem::default()];
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.first_run_requested());

    let info = CaptureEvidence::first_run_fixture_info();
    assert_eq!(
        info.path,
        std::path::Path::new("/usr/share/taskforest/setup/99-taskforest.rules")
    );
    assert_eq!(
        info.run_command,
        "pkexec /usr/libexec/taskforest-setup-helper install"
    );
    assert_eq!(
        info.revert_command,
        "pkexec /usr/libexec/taskforest-setup-helper revert"
    );

    evidence.mark_first_run_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_first_run_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.first_run_requested());
}

#[test]
fn apps_zero_gray_capture_keeps_zero_values_measured_and_fixture_bounded() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::AppsZeroGray));
    assert!(evidence.apps_zero_gray_enabled());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();

    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.scenario_ready);
    assert_eq!(processes.len(), 2);
    assert!(processes.iter().all(|process| {
        process.current_cpu_percentage() == Some(0.0)
            && process.current_memory_pss_bytes() == Some(0)
            && process.current_swap_bytes() == Some(0)
            && process.current_disk_read_bytes_per_sec() == Some(0)
            && process.current_disk_write_bytes_per_sec() == Some(0)
    }));

    let before = processes.len();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert_eq!(processes.len(), before, "fixture refresh must stay bounded");
}

#[test]
fn apps_group_capture_keeps_a_bounded_expanded_fixture_after_refresh() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::AppsGroupExpanded));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();

    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.apps_group_expanded_requested());
    assert_eq!(processes.len(), 3);
    assert!(
        processes
            .iter()
            .all(|process| process.name == "capture-browser")
    );
    assert!(processes.iter().all(|process| {
        process
            .current_application_identity()
            .is_some_and(|identity| identity.has_icon_asset())
    }));

    evidence.mark_apps_group_expanded_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_apps_group_expanded_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.apps_group_expanded_requested());

    let before = processes.len();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert_eq!(processes.len(), before, "fixture refresh must stay bounded");
}

#[test]
fn apps_identity_matrix_fixture_keeps_three_validated_target_shapes() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::AppsIdentityMatrix));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();

    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.apps_identity_matrix_requested());
    assert_eq!(processes.len(), 3);
    for (name, launcher_id, executable_fragment) in [
        ("chrome", "chrome-mail.desktop", "/opt/google/chrome/chrome"),
        ("snap", "snap.firefox_firefox.desktop", "/usr/bin/snap"),
        (
            "AppRun",
            "portable-editor.desktop",
            "/tmp/.mount_PortableEditor-abc123/AppRun",
        ),
    ] {
        let process = processes
            .iter()
            .find(|process| process.name == name)
            .expect("identity matrix process fixture");
        assert!(process.cmdline.contains(executable_fragment));
        let identity = process
            .current_application_identity()
            .expect("identity matrix must publish an application identity");
        assert_eq!(identity.launcher_id, launcher_id);
        assert!(identity.has_icon_asset());
    }

    evidence.mark_apps_identity_matrix_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_apps_identity_matrix_ready(true);
    assert!(evidence.scenario_ready);
    let before = processes.len();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert_eq!(
        processes.len(),
        before,
        "identity fixture must stay bounded"
    );
}

#[test]
fn health_scenarios_wait_for_exact_visible_fixture_state() {
    for scenario in [
        CaptureScenario::StorageHealth,
        CaptureScenario::SmartSelfTestConfirm,
        CaptureScenario::SensorCenter,
    ] {
        let mut evidence = CaptureEvidence::for_test(Some(scenario));
        assert!(evidence.system_health_fixture_requested());
        let mut snapshot = SystemSnapshot::default();
        evidence.on_snapshot(&mut snapshot);
        let mut processes = Vec::new();
        assert!(evidence.on_processes_update(true, &mut processes).is_none());
        let mut page = TopPage::Apps;
        let mut dashboard = DashboardState::new();
        let mut filesystems = taskmanager_core::core::FilesystemHealthSnapshot::default();
        let mut sensors = taskmanager_core::core::SensorCenterSnapshot::default();
        let outcome = evidence.on_system_health_state(
            &mut page,
            &mut dashboard,
            &mut snapshot,
            &mut filesystems,
            &mut sensors,
        );
        assert!(outcome.ready());
        assert_eq!(page, TopPage::System);
        assert_eq!(dashboard.section, SystemSection::Health);
        let selected_disk = &snapshot.disks[0];
        assert!(
            evidence
                .system_health_report_for(
                    &selected_disk.device_id,
                    selected_disk.device_generation,
                )
                .is_some(),
            "capture keeps one complete typed observation for the visible disk"
        );
        assert_eq!(
            matches!(
                outcome,
                SystemHealthCaptureOutcome::ReadyWithConfirmation(_)
            ),
            scenario == CaptureScenario::SmartSelfTestConfirm
        );
        assert!(evidence.scenario_ready);
    }
}

#[test]
fn dynamic_device_capture_installs_battery_and_fan_fixture_after_readiness() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::BatteryFanPerformance));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    let mut page = TopPage::Apps;
    let mut power_supplies = taskmanager_core::core::PowerSupplySnapshot::default();
    let mut sensors = taskmanager_core::core::SensorCenterSnapshot::default();
    assert!(evidence.on_dynamic_device_state(&mut page, &mut power_supplies, &mut sensors,));
    assert_eq!(page, TopPage::Performance);
    assert_eq!(power_supplies.batteries.len(), 1);
    assert!(
        sensors
            .readings
            .iter()
            .any(|reading| reading.quantity() == &taskmanager_core::core::SensorQuantity::FanSpeed)
    );
    assert!(evidence.scenario_ready);
    assert!(!evidence.on_dynamic_device_state(&mut page, &mut power_supplies, &mut sensors,));
}

#[test]
fn partition_capture_installs_mounted_and_unmounted_children() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::PartitionDiskUsage));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);

    assert!(evidence.scenario_ready);
    assert_eq!(snapshot.disks.len(), 1);
    assert_eq!(snapshot.disks[0].partitions.len(), 3);
    assert_eq!(
        snapshot.disks[0].partitions[0].current_used_bytes(),
        Some(600 * 1024 * 1024 * 1024)
    );
    assert_eq!(
        snapshot.disks[0].partitions[1].mount_point,
        "/mnt/capture/long-mount-point-for-layout-regression/home"
    );
    assert_eq!(snapshot.disks[0].partitions[2].current_used_bytes(), None);
    assert_eq!(snapshot.disks[0].partitions[2].current_free_bytes(), None);
}

#[test]
fn process_memory_capture_fixture_keeps_pss_and_swap_separate() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessMemoryPssSwap));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();

    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.process_memory_pss_swap_requested());
    assert_eq!(processes.len(), 3);
    let browser = processes
        .iter()
        .find(|process| process.name == "capture-browser")
        .expect("PSS capture fixture must contain the browser row");
    assert_eq!(browser.current_memory_bytes(), Some(768 * 1024 * 1024));
    assert_eq!(browser.current_memory_pss_bytes(), Some(410 * 1024 * 1024));
    assert_eq!(browser.current_swap_bytes(), Some(96 * 1024 * 1024));

    let before = processes.len();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert_eq!(processes.len(), before, "fixture refresh must stay bounded");
}

#[test]
fn smart_scenario_prepares_explicit_missing_tool_state() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SmartMissingTool));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert!(evidence.telemetry_ready);
    assert!(evidence.scenario_ready);
    assert_eq!(snapshot.disks.len(), 1);
    let disk = &snapshot.disks[0];
    assert_eq!(disk.smart_availability, SmartAvailability::MissingTool);
    assert!(disk.smart_temperature_c.is_none());
    assert!(disk.smart_critical_warning.is_none());
    assert!(disk.smart_power_on_hours.is_none());
}

#[test]
fn mc02_hotplug_case_hotplug_capture_exposes_disconnect_after_stable_identity_was_seen() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::DeviceHotplug));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert_eq!(snapshot.disks[0].device_id, "disk:wwid:capture-hotplug");
    assert_eq!(snapshot.disks[0].device_generation.get(), 1);
    assert!(!evidence.scenario_ready);
    evidence.on_snapshot(&mut snapshot);
    assert!(snapshot.disks.is_empty());
    assert!(evidence.scenario_ready);

    // The disconnect projection remains stable while the screenshot runner
    // settles. A later provider refresh must then publish the same stable
    // identity as a new generation. This is the recovery half of the hot-plug
    // contract; the screenshot marker remains on the disconnected frame.
    for _ in 0..3 {
        evidence.on_snapshot(&mut snapshot);
        assert!(snapshot.disks.is_empty());
    }
    evidence.on_snapshot(&mut snapshot);
    assert_eq!(snapshot.disks.len(), 1);
    assert_eq!(snapshot.disks[0].device_id, "disk:wwid:capture-hotplug");
    assert_eq!(snapshot.disks[0].device_generation.get(), 2);
}

#[test]
fn intel_and_alert_capture_states_keep_optional_values_honest() {
    let mut intel = CaptureEvidence::for_test(Some(CaptureScenario::IntelGpuTelemetry));
    let mut snapshot = SystemSnapshot::default();
    intel.on_snapshot(&mut snapshot);
    let gpu = &snapshot.gpu[0];
    assert_eq!(gpu.current_utilization_pct(), Some(37.0));
    assert_eq!(gpu.current_frequency_mhz(), Some(1_850));
    assert_eq!(gpu.current_idle_residency_pct(), Some(62.0));
    assert_eq!(gpu.current_temperature_c(), None);
    assert_eq!(gpu.current_power_w(), None);
    assert_eq!(gpu.current_memory_total_bytes(), None);
    let mut alert = CaptureEvidence::for_test(Some(CaptureScenario::ActiveAlert));
    alert.on_snapshot(&mut snapshot);
    assert_eq!(snapshot.disks[0].smart_critical_warning, Some(true));
}

#[test]
fn gpu_engine_inventory_capture_seeds_five_typed_aggregate_and_engine_frames() {
    use crate::gpui_app::history_samples::gpu_engine_samples;
    use taskmanager_core::core::DeviceGeneration;
    use taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetric;
    use taskmanager_telemetry_store::TelemetryStore;
    use taskmanager_telemetry_store::live_graph::LiveGraphHistory;

    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::GpuEngineInventory));
    let mut snapshot = SystemSnapshot {
        timestamp_ms: 10_000,
        ..SystemSnapshot::default()
    };
    evidence.on_snapshot(&mut snapshot);
    let gpu = &snapshot.gpu[0];
    assert_eq!(gpu.device_id, "gpu:capture:engine-inventory");
    assert_eq!(gpu.device_generation, DeviceGeneration::new(1));
    assert_eq!(gpu.engines.len(), 2);
    assert!(!evidence.scenario_ready);

    let mut processes = Vec::new();
    evidence.on_processes_update(true, &mut processes);
    let (store, ingestor) = TelemetryStore::shared_with_correlated_ingestion(32);
    let live_graph = LiveGraphHistory::from_store(store.clone(), 32);
    assert!(evidence.seed_gpu_engine_inventory_history(
        &store.system_history,
        &ingestor,
        snapshot.timestamp_ms,
    ));
    assert!(evidence.scenario_ready);
    let aggregate_samples =
        taskmanager_shell::presentation::gpu_chart_metric::gpu_chart_metric_history(
            &live_graph,
            "gpu:capture:engine-inventory",
            1,
            GpuChartMetric::Utilization,
        );
    assert_eq!(aggregate_samples.len(), 5);
    assert_eq!(
        aggregate_samples.last().copied(),
        gpu.current_utilization_pct(),
        "the captured current aggregate fact must equal the last graphed sample"
    );
    for engine in ["Render/3D", "Video Decode"] {
        let samples = gpu_engine_samples(
            &store.system_history,
            "gpu:capture:engine-inventory",
            DeviceGeneration::new(1),
            engine,
        );
        assert_eq!(samples.len(), 5, "{engine} must retain every capture frame");
        assert_eq!(
            samples.last().copied(),
            gpu.engines
                .iter()
                .find(|candidate| candidate.name == engine)
                .map(|candidate| candidate.usage_pct),
            "the captured current {engine} fact must equal the last graphed sample"
        );
    }
}

#[test]
fn system_npu_capture_waits_for_fixture_layout_and_visible_scroll_before_marker() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::SystemNpu));
    let mut snapshot = SystemSnapshot::default();
    let mut processes = Vec::new();
    evidence.on_snapshot(&mut snapshot);
    evidence.on_processes_update(true, &mut processes);

    let fixture = evidence
        .system_hardware_npu_fixture()
        .expect("system NPU capture installs the typed NPU fixture");
    assert!(fixture.is_success());
    evidence.mark_system_npu_fixture_ready(true);
    assert!(evidence.system_npu_layout_requested());
    assert!(evidence.schedule_system_npu_scroll());
    assert!(!evidence.schedule_system_npu_scroll());
    evidence.mark_system_npu_scroll_applied(false);
    assert!(evidence.system_npu_layout_requested());

    assert!(evidence.schedule_system_npu_scroll());
    evidence.mark_system_npu_scroll_applied(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.system_npu_layout_requested());
}

#[test]
fn process_and_service_capture_actions_are_typed_and_non_destructive() {
    let mut properties =
        CaptureEvidence::for_test(Some(CaptureScenario::ProcessPropertiesPerformance));
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(42)
            .name("capture-process".into())
            .build(),
    ];
    assert_eq!(
        properties.on_processes_update(true, &mut processes),
        Some(CaptureProcessAction::Properties(
            42,
            ProcessDetailsSection::Performance
        ))
    );
    assert_eq!(processes[0].cpu_history.len(), 60);
    let mut tree = CaptureEvidence::for_test(Some(CaptureScenario::ProcessTreeConfirm));
    let action = tree.on_processes_update(true, &mut processes).unwrap();
    let CaptureProcessAction::Termination(intent) = action else {
        panic!("expected tree confirmation")
    };
    assert_eq!(intent.action, ProcessTerminationAction::EndProcessTree);
    assert_eq!(intent.descendant_count(), 6);
    let mut service = CaptureEvidence::for_test(Some(CaptureScenario::ServiceDetailsLogs));
    let mut services = Vec::new();
    assert_eq!(
        service.on_services_update(true, &mut services),
        Some(ServiceId::new(
            "fixture.service:taskmanager-capture.service"
        ))
    );
    assert_eq!(services.len(), 1);
    assert!(!service.scenario_ready);
    service.mark_service_details_ready(true);
    assert!(service.scenario_ready);
}

#[test]
fn insights_scenarios_wait_for_exact_dialog_state_and_never_create_control_intents() {
    for scenario in [
        CaptureScenario::ProcessNetworkDetails,
        CaptureScenario::ProcessGpuDetails,
        CaptureScenario::ProcessResourceLimits,
        CaptureScenario::ProcessIsolation,
    ] {
        let mut evidence = CaptureEvidence::for_test(Some(scenario));
        let mut snapshot = SystemSnapshot::default();
        evidence.on_snapshot(&mut snapshot);
        let mut processes = Vec::new();
        let action = evidence
            .on_processes_update(true, &mut processes)
            .expect("strict insights scenario should prepare a render fixture");
        let CaptureProcessAction::Insights { pid, state } = action else {
            panic!("insights captures must not create a process-control intent")
        };
        assert_eq!(pid, 4242);
        assert!(processes.iter().any(|process| process.pid == pid));
        assert!(matches!(state, ProcessInsightsState::Ready(_)));
        assert!(!evidence.scenario_ready);
        evidence.mark_process_insights_ready(false);
        assert!(!evidence.scenario_ready);
        evidence.mark_process_insights_ready(true);
        assert!(evidence.scenario_ready);
        processes.clear();
        assert!(evidence.on_processes_update(true, &mut processes).is_none());
        assert!(processes.iter().any(|process| process.pid == pid));
    }
}

#[test]
fn batch_capture_freezes_three_identities_without_executing() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessBatchConfirm));
    let mut processes = Vec::new();
    let action = evidence.on_processes_update(true, &mut processes).unwrap();
    let CaptureProcessAction::Batch(intent) = action else {
        panic!("expected typed batch confirmation")
    };
    assert_eq!(intent.action, ProcessBatchAction::Suspend);
    assert_eq!(
        intent
            .targets
            .iter()
            .map(|target| target.pid)
            .collect::<Vec<_>>(),
        [91_001, 91_002, 91_003]
    );
    assert!(
        intent
            .targets
            .iter()
            .all(|target| target.start_time_secs > 0)
    );
    assert!(!evidence.scenario_ready);
    evidence.mark_process_batch_ready(true, intent.targets.len());
    assert!(evidence.scenario_ready);
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
}

#[test]
fn startup_capture_distinguishes_measured_from_unknown() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::StartupImpact));
    let mut entries = Vec::new();
    let mut boot_evidence = None;
    assert!(!evidence.on_startup_update(false, &mut entries, &mut boot_evidence));
    assert!(evidence.on_startup_update(true, &mut entries, &mut boot_evidence));
    // The impact-only scenario seeds the list but never the waterfall pair.
    assert!(boot_evidence.is_none());
    assert!(evidence.startup_boot_baseline().is_none());
    assert_eq!(entries.len(), 2);
    assert!(matches!(
        entries[0].impact_evidence,
        StartupImpactEvidence::Measured { duration_ms: 842 }
    ));
    assert!(matches!(
        entries[1].impact_evidence,
        StartupImpactEvidence::Unknown {
            reason: StartupImpactUnknownReason::NotInstrumented
        }
    ));
    assert!(!evidence.scenario_ready);
    evidence.mark_startup_impact_ready(true, &entries);
    assert!(evidence.scenario_ready);
}

#[test]
fn startup_failure_evidence_capture_seeds_failed_units_and_chain() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::StartupFailureEvidence));
    let mut entries = Vec::new();
    let mut boot_evidence = None;
    assert!(evidence.on_startup_update(true, &mut entries, &mut boot_evidence));
    let snapshot = boot_evidence.clone().expect("failure evidence fixture");
    assert_eq!(snapshot.failed_units.len(), 3);
    assert_eq!(snapshot.critical_chain.len(), 2);
    assert!(evidence.startup_boot_baseline().is_none());
    assert!(!evidence.scenario_ready);
    evidence.restore_startup_fixture(&mut entries, &mut boot_evidence);
    let restored = boot_evidence.expect("fixture must survive a later platform batch");
    assert_eq!(restored.failed_units.len(), 3);
    evidence.mark_startup_failure_evidence_ready(true, Some(&snapshot));
    assert!(evidence.scenario_ready);
}

#[test]
fn boot_markers_capture_seeds_waterfall_and_baseline_pair() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::StartupBootMarkers));
    let mut entries = Vec::new();
    let mut boot_evidence = None;
    assert!(evidence.on_startup_update(true, &mut entries, &mut boot_evidence));
    // The waterfall evidence carries a measured critical chain...
    let evidence_snapshot =
        boot_evidence.expect("markers scenario seeds the boot evidence snapshot");
    assert_eq!(evidence_snapshot.critical_chain.len(), 3);
    assert!(evidence_snapshot.critical_chain_failure.is_none());
    // ...and the baseline covers the same units, so segment deltas exist and
    // cover all three chip states (slower / faster / unchanged).
    let baseline = evidence
        .startup_boot_baseline()
        .expect("markers scenario seeds the comparison baseline");
    let units: Vec<&str> = baseline
        .segments
        .iter()
        .map(|segment| segment.unit.as_str())
        .collect();
    assert_eq!(units.len(), 3);
    for unit in evidence_snapshot
        .critical_chain
        .iter()
        .map(|n| n.unit.as_str())
    {
        assert!(units.contains(&unit), "baseline must cover unit {unit}");
    }
    let current = taskmanager_core::core::startup::BootTimeline::from_critical_chain(
        &evidence_snapshot.critical_chain,
        taskmanager_core::core::startup::DEFAULT_BOOT_TIMELINE_MAX_SEGMENTS,
        taskmanager_core::core::startup::DEFAULT_BOOT_TIMELINE_MAX_UNTIMED,
    );
    let deltas: Vec<i64> = taskmanager_core::core::startup::segment_deltas(&current, baseline)
        .into_iter()
        .map(|delta| delta.delta_ms)
        .collect();
    assert!(deltas.contains(&200), "one unit slower: {deltas:?}");
    assert!(deltas.contains(&-300), "one unit faster: {deltas:?}");
    assert!(deltas.contains(&0), "one unit unchanged: {deltas:?}");
    // Readiness requires BOTH the page and the seeded pair.
    assert!(!evidence.scenario_ready);
    evidence.mark_startup_boot_markers_ready(true, false);
    assert!(!evidence.scenario_ready);
    evidence.mark_startup_boot_markers_ready(true, true);
    assert!(evidence.scenario_ready);
}

#[test]
fn history_replay_capture_opens_once_and_marks_ready_only_when_loaded() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::HistoryReplay));
    // Before readiness the open request must stay refused.
    assert!(!evidence.history_replay_open_requested());
    evidence.telemetry_ready = true;
    assert!(!evidence.history_replay_open_requested());
    evidence.ui_data_ready = true;
    assert!(evidence.history_replay_open_requested());
    // Opening latches: the request never fires twice (the panel must not be
    // toggled closed again on a later tick).
    evidence.note_history_replay_opened();
    assert!(!evidence.history_replay_open_requested());
    // Readiness needs rows actually loaded, not just the panel open.
    evidence.mark_history_replay_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_history_replay_ready(true);
    assert!(evidence.scenario_ready);
}

#[test]
fn application_history_capture_requires_ready_non_empty_durable_projection() {
    use taskmanager_application::ApplicationHistoryStatus;

    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ApplicationHistoryReplay));
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Ready, 3);
    assert!(
        !evidence.scenario_ready,
        "normal capture facts are still pending"
    );

    evidence.telemetry_ready = true;
    evidence.ui_data_ready = true;
    evidence.mark_application_history_replay_ready(false, ApplicationHistoryStatus::Ready, 3);
    assert!(
        !evidence.scenario_ready,
        "the application-history page must be active"
    );
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Collecting, 3);
    assert!(
        !evidence.scenario_ready,
        "an active empty reader is not replay evidence"
    );
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Ready, 0);
    assert!(
        !evidence.scenario_ready,
        "Ready without joined rows is not evidence"
    );
    evidence.mark_application_history_replay_ready(true, ApplicationHistoryStatus::Ready, 3);
    assert!(evidence.scenario_ready);
}

#[test]
fn diagnostic_capture_requests_preview_but_never_confirms_write() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::DiagnosticPreview));
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert_eq!(processes.len(), 1);
    assert!(processes[0].cmdline.contains("/home/<user>"));
    assert!(evidence.diagnostic_preview_requested());
    evidence.mark_diagnostic_preview_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_diagnostic_preview_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.diagnostic_preview_requested());
}

#[test]
fn diagnostic_failure_capture_prepares_ui_state_without_worker_action() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::DiagnosticFailure));
    assert!(!evidence.diagnostic_failure_requested());
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    let mut processes = Vec::new();
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
    assert!(evidence.diagnostic_failure_requested());
    evidence.mark_diagnostic_failure_ready(false);
    assert!(!evidence.scenario_ready);
    evidence.mark_diagnostic_failure_ready(true);
    assert!(evidence.scenario_ready);
    assert!(!evidence.diagnostic_failure_requested());
}

#[test]
fn force_kill_scenario_only_returns_one_non_executing_intent() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessForceKill));
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(1)
            .name("init".into())
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(4242)
            .name("capture-worker".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(42_420, 1),
                ..Default::default()
            })
            .build(),
    ];
    assert!(
        evidence
            .on_processes_update(false, &mut processes)
            .is_none()
    );
    assert_eq!(
        evidence.on_processes_update(true, &mut processes),
        Some(CaptureProcessAction::Termination(
            crate::gpui_app::root::termination::snapshot_single_process(
                ProcessTerminationAction::ForceKill,
                4242,
                &processes,
            )
            .expect("capture fixture has an authoritative start token")
        ))
    );
    assert!(evidence.ui_data_ready);
    assert!(evidence.scenario_ready);
    assert!(evidence.on_processes_update(true, &mut processes).is_none());
}

#[test]
fn force_kill_capture_prefers_a_readable_process_name() {
    let mut evidence = CaptureEvidence::for_test(Some(CaptureScenario::ProcessForceKill));
    let mut processes = vec![
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(20)
            .name("worker/u65:0-btrfs-endio-meta".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(2_000, 1),
                ..Default::default()
            })
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(30)
            .name("bash".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(3_000, 1),
                ..Default::default()
            })
            .build(),
        taskmanager_test_support::ProcessItemFixtureBuilder::new()
            .pid(40)
            .name("taskmanager".into())
            .scalar_observations(taskmanager_core::core::process::ProcessScalarObservations {
                start_token: ScalarObservation::available(4_000, 1),
                ..Default::default()
            })
            .build(),
    ];
    let action = evidence.on_processes_update(true, &mut processes).unwrap();
    let CaptureProcessAction::Termination(intent) = action else {
        panic!("expected termination capture action")
    };
    assert_eq!(intent.root.pid, 40);
    assert_eq!(intent.action, ProcessTerminationAction::ForceKill);
}

#[test]
fn standard_evidence_marks_updates_without_mutating_data() {
    let mut evidence = CaptureEvidence::for_test(None);
    let mut snapshot = SystemSnapshot::default();
    evidence.on_snapshot(&mut snapshot);
    assert!(snapshot.disks.is_empty());
    assert!(evidence.telemetry_ready);
    assert!(
        evidence
            .on_processes_update(true, &mut Vec::new())
            .is_none()
    );
    assert!(evidence.ui_data_ready);
    assert!(!evidence.scenario_ready);
}
