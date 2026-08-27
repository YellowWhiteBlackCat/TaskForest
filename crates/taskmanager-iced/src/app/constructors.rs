//! Constructors for [`super::IcedApp`]: the blank no-platform launch path, the
//! deterministic demo fixture, and the injected-config-store test seam.
//! Extracted from [`super`] so the state + update module stays the entry point.

use std::path::PathBuf;

use taskmanager_application::PlatformClient;

use super::*;

fn default_category_expansions() -> std::collections::HashSet<String> {
    taskmanager_application::ProcessCategory::ALL
        .iter()
        .copied()
        .map(taskmanager_application::process_category_projection::category_expansion_key)
        .collect()
}

impl IcedApp {
    /// Build a blank frontend; `platform` is `None` for no-I/O loading-state
    /// tests. Use [`Self::demo`] when fixture data should be visible.
    #[must_use]
    pub fn new(platform: Option<PlatformClient>) -> Self {
        Self::new_with_runtime_clients(platform, None, None)
    }

    pub(crate) fn new_with_runtime_clients(
        platform: Option<PlatformClient>,
        config_client: Option<ConfigClient>,
        history_replay_client: Option<taskmanager_app_host::HistoryReplayClient>,
    ) -> Self {
        Self::new_with_native_runtime_clients(
            platform,
            config_client,
            history_replay_client,
            taskmanager_application::LocalTimeRulesObservation::unsupported(0),
        )
    }

    pub(crate) fn new_with_native_runtime_clients(
        platform: Option<PlatformClient>,
        config_client: Option<ConfigClient>,
        history_replay_client: Option<taskmanager_app_host::HistoryReplayClient>,
        local_time_rules: taskmanager_application::LocalTimeRulesObservation,
    ) -> Self {
        let mut shell = ShellApp::new();
        if let Some(platform) = platform.as_ref() {
            shell.apply_capability_snapshot(platform.capabilities().snapshot());
        }
        let mut app = Self {
            shell,
            local_time_rules,
            runtime: super::runtime::IcedRuntime::new(platform),
            capture: super::capture_state::CaptureState::new(capture_marker_path()),
            input: super::input_state::InputState::default(),
            configuration: super::configuration_state::IcedConfiguration::new(
                config_client,
                crate::font_catalog::system(),
            ),
            run_task: crate::ui::overlays::run_task::RunTaskState::default(),
            saved_views: crate::saved_views::default_built_in_presets(),
            next_saved_view_id: 10,
            saved_view_feedback: None,
            alert_center: crate::ui::overlays::alerts::AlertCenterState::default(),
            alerts_page: alerts::AlertsPageState::default(),
            first_run: crate::ui::first_run::FirstRunUiState::default(),
            first_run_requests: std::collections::HashMap::new(),
            system_dashboard_window: taskmanager_application::HistoryWindow::OneHour,
            history_runtime: super::history_replay::IcedHistoryRuntime::new(history_replay_client),
            snapshot_export: super::snapshot_export::IcedSnapshotExportRuntime::default(),
            local_surface: LocalSurfaceState::default(),
            process_presentation: super::process_presentation_state::ProcessPresentationState::new(
                default_category_expansions(),
            ),
            process_column_sizing: ProcessColumnSizing::default(),
            service_log_export: super::service_log::IcedServiceLogExportRuntime::default(),
            service_details: service_details::ServiceDetailsState::default(),
            performance: super::performance_state::PerformanceState::default(),
            window_time: super::window_time::WindowTimeCache::default(),
            viewport: super::viewport_state::IcedViewportState::new(
                crate::run::initial_window_size(),
            ),
            projection_caches: IcedProjectionCaches::default(),
        };
        // The boot observation is the dialog's trigger (GPUI parity): it is
        // submitted through the platform channel before the first frame, and
        // its correlated answer on the tick lane decides visibility. A
        // missing platform folds the honest hidden state with no notice.
        app.begin_first_run_observation();
        // The shared catalog is pinned to this frontend's language at the
        // runtime edges — `load_config` for real launches and the demo boot
        // closure in run.rs — never in the constructors, so parallel headless
        // tests cannot race the process-wide catalog global through them.
        app
    }

    /// Build a deterministic no-I/O frontend with the shared shell fixture.
    /// The binary uses this only for `--demo`; ordinary `new(None)` remains a
    /// blank no-platform state for loading-state tests.
    #[must_use]
    pub fn demo() -> Self {
        Self {
            shell: {
                // Seed the shared boot-evidence slot (the TUI demo seeds the
                // same field through `shell.projection()`).
                let mut shell = taskmanager_shell::demo_app();
                taskmanager_shell::fixture::seed_projection_fact(
                    &mut shell,
                    taskmanager_shell::fixture::ProjectionSeedFact::StartupBootEvidence(Some(
                        demo_boot_evidence(),
                    )),
                );
                shell
            },
            local_time_rules: taskmanager_application::LocalTimeRulesObservation::current(
                taskmanager_application::LocalTimeRules::utc(),
                0,
            ),
            runtime: super::runtime::IcedRuntime::new(None),
            capture: super::capture_state::CaptureState::new(capture_marker_path()),
            input: super::input_state::InputState::default(),
            configuration: super::configuration_state::IcedConfiguration::new(
                None,
                crate::font_catalog::bundled_only(),
            ),
            run_task: crate::ui::overlays::run_task::RunTaskState::default(),
            saved_views: crate::saved_views::default_built_in_presets(),
            next_saved_view_id: 10,
            saved_view_feedback: None,
            alert_center: crate::ui::overlays::alerts::AlertCenterState::default(),
            alerts_page: alerts::AlertsPageState::default(),
            // The demo has no platform client, so the boot observation is
            // skipped: the dialog stays hidden (there is no asset answer to
            // wait for and none is fabricated).
            first_run: crate::ui::first_run::FirstRunUiState::default(),
            first_run_requests: std::collections::HashMap::new(),
            system_dashboard_window: taskmanager_application::HistoryWindow::OneHour,
            history_runtime: super::history_replay::IcedHistoryRuntime::new(None),
            snapshot_export: super::snapshot_export::IcedSnapshotExportRuntime::default(),
            local_surface: LocalSurfaceState::default(),
            process_presentation: super::process_presentation_state::ProcessPresentationState::new(
                default_category_expansions(),
            ),
            process_column_sizing: ProcessColumnSizing::default(),
            service_log_export: super::service_log::IcedServiceLogExportRuntime::default(),
            service_details: service_details::ServiceDetailsState::default(),
            performance: super::performance_state::PerformanceState::default(),
            window_time: super::window_time::WindowTimeCache::default(),
            viewport: super::viewport_state::IcedViewportState::new(
                crate::run::initial_window_size(),
            ),
            projection_caches: IcedProjectionCaches::default(),
        }
        // The demo keeps the shared shell fixture's single recorded snapshot
        // (G-02): the Performance chart renders the honest "collecting"
        // placeholder instead of the retired ring's synthetic wave — seeding a
        // fabricated window into the shared store would also flip the alert
        // suggestion overlay above its sample floor on synthetic data.
        // Real pixel capture uses `demo_for_capture`, which seeds multi-sample
        // synthetic snapshots into the same store for the visual branches.
    }

    /// Build the deterministic demo shape for an evidence run. The optional
    /// environment selector is deliberately a fixed vocabulary so capture can
    /// target a device page without adding a production command or arbitrary
    /// state injection path.
    pub(crate) fn demo_for_capture() -> Self {
        let mut app = Self::demo();
        app.process_presentation.expanded_groups = default_category_expansions();
        seed_capture_performance_fixture(&mut app);
        if let Some(target) = std::env::var_os("TM_ICED_CAPTURE_DEVICE")
            .and_then(|value| value.to_str().map(str::to_owned))
        {
            apply_capture_target(&mut app, &target);
        }
        seed_capture_source_failure(&mut app);
        app
    }
}

/// Apply one fixed capture target and its page-local facts. System capture is
/// the only route that receives the synthetic NPU inventory; ordinary demos
/// and every other evidence page keep the real unobserved state.
fn apply_capture_target(app: &mut IcedApp, target: &str) {
    if target == "service-details" {
        app.shell.application.active_page = taskmanager_application::AppPage::Services;
        let _ = app.open_service_details_for_effect(0);
    } else if let Some(page) = capture_page_from_name(target) {
        app.shell.application.active_page = page;
        if page == taskmanager_application::AppPage::System {
            seed_capture_npu_fixture(app);
        }
    } else if let Some(device) = capture_device_from_name(target) {
        app.performance.selected_device = device;
    }
}

fn seed_capture_npu_fixture(app: &mut IcedApp) {
    let observed_at_ms = 7_000;
    let inventory = taskmanager_application::NpuInventorySnapshot::discovered(
        vec![taskmanager_application::NpuDevice {
            device_id: taskmanager_application::DeviceId::new("accel0"),
            brand: Some("Intel AI Boost".into()),
            driver: Some("intel_vpu".into()),
            utilization_pct: taskmanager_application::ScalarObservation::available(
                38.0,
                observed_at_ms,
            ),
            engines: vec![taskmanager_application::NpuEngineUsage {
                kind: taskmanager_application::NpuEngineKind::Matrix,
                utilization_pct: taskmanager_application::ScalarObservation::available(
                    61.0,
                    observed_at_ms,
                ),
            }],
            memory: taskmanager_application::NpuMemoryReport {
                dedicated_total_bytes: taskmanager_application::ScalarObservation::available(
                    0,
                    observed_at_ms,
                ),
                shared_total_bytes: taskmanager_application::ScalarObservation::unavailable(
                    taskmanager_application::FailureKind::Unsupported,
                ),
            },
            ..Default::default()
        }],
        observed_at_ms,
    );
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::NpuInventory(Some(inventory)),
    );
}

/// Enrich the no-I/O demo only for real pixel capture. The ordinary
/// `IcedApp::demo()` fixture remains intentionally small for loading-state
/// tests; the capture fixture must nevertheless exercise the same visual
/// branches GPUI demonstrates: multi-sample device graphs, battery/fan cards,
/// the fixed CPU histories and per-core grid. Every value here is deterministic and
/// stays inside the frontend fixture boundary.
fn seed_capture_performance_fixture(app: &mut IcedApp) {
    let Some(base) = app.shell.projection().snapshot.clone() else {
        return;
    };

    for (index, (cpu, memory, disk_rate, network_rate, gpu, temperature, frequency, power)) in [
        (31.0, 41.0, 68.0, 44.0, 16.0, 47.0, 2_450_u64, 18.0),
        (44.0, 46.0, 92.0, 61.0, 24.0, 53.0, 3_050, 31.0),
        (37.0, 43.0, 54.0, 28.0, 19.0, 50.0, 2_780, 24.0),
        (52.0, 49.0, 116.0, 73.0, 31.0, 58.0, 3_280, 42.0),
        (39.0, 45.0, 81.0, 52.0, 22.0, 51.0, 2_940, 27.0),
    ]
    .into_iter()
    .enumerate()
    {
        let mut snapshot = base.clone();
        snapshot.timestamp_ms += (index as u64 + 1) * 1_000;
        let core_usages = [cpu * 1.25, cpu * 0.9, cpu * 0.72, cpu * 0.48]
            .into_iter()
            .collect();
        snapshot
            .cpu
            .apply_scalar_observations(taskmanager_application::CpuScalarObservations {
                global_usage_pct: taskmanager_application::ScalarObservation::available(
                    cpu,
                    snapshot.timestamp_ms,
                ),
                core_usage_group: taskmanager_application::ScalarObservationGroup::available(
                    core_usages,
                    snapshot.timestamp_ms,
                ),
                frequency_mhz: taskmanager_application::ScalarObservation::available(
                    frequency,
                    snapshot.timestamp_ms,
                ),
                temperature_c: taskmanager_application::ScalarObservation::available(
                    temperature,
                    snapshot.timestamp_ms,
                ),
                power_w: taskmanager_application::ScalarObservation::available(
                    power,
                    snapshot.timestamp_ms,
                ),
                ..Default::default()
            });
        if let Some(total_bytes) = snapshot.memory.current_total_bytes() {
            let mut observations = *snapshot.memory.scalar_observations();
            observations.used_bytes = taskmanager_application::ScalarObservation::available(
                (total_bytes as f32 * memory / 100.0) as u64,
                snapshot.timestamp_ms,
            );
            let optional_observations = snapshot.memory.optional_observations().clone();
            snapshot
                .memory
                .apply_observations(observations, optional_observations);
        }
        if let Some(disk) = snapshot.disks.first_mut() {
            let mut observations = *disk.scalar_observations();
            observations.read_bytes_per_sec = taskmanager_application::ScalarObservation::available(
                (disk.current_read_bytes_per_sec().unwrap_or_default() as f32 * disk_rate / 84.0)
                    as u64,
                snapshot.timestamp_ms,
            );
            observations.write_bytes_per_sec =
                taskmanager_application::ScalarObservation::available(
                    (disk.current_write_bytes_per_sec().unwrap_or_default() as f32 * disk_rate
                        / 84.0) as u64,
                    snapshot.timestamp_ms,
                );
            disk.apply_scalar_observations(observations);
        }
        if let Some(network) = snapshot.networks.first_mut() {
            let mut observations = *network.scalar_observations();
            observations.rx_bytes_per_sec = taskmanager_application::ScalarObservation::available(
                (network.current_rx_bytes_per_sec().unwrap_or_default() as f32 * network_rate
                    / 12.0) as u64,
                snapshot.timestamp_ms,
            );
            observations.tx_bytes_per_sec = taskmanager_application::ScalarObservation::available(
                (network.current_tx_bytes_per_sec().unwrap_or_default() as f32 * network_rate
                    / 12.0) as u64,
                snapshot.timestamp_ms,
            );
            let adapter_type = network.adapter_type();
            let wireless_observations = network.wireless_observations().clone();
            network.apply_observations(adapter_type, observations, wireless_observations);
        }
        if let Some(gpu_metrics) = snapshot.gpu.first_mut() {
            gpu_metrics.apply_scalar_observations(taskmanager_application::GpuScalarObservations {
                utilization_pct: taskmanager_application::ScalarObservation::available(
                    gpu,
                    snapshot.timestamp_ms,
                ),
                temperature_c: taskmanager_application::ScalarObservation::available(
                    43.0 + gpu / 5.0,
                    snapshot.timestamp_ms,
                ),
                frequency_mhz: taskmanager_application::ScalarObservation::available(
                    (600.0 + gpu * 18.0) as u64,
                    snapshot.timestamp_ms,
                ),
                ..Default::default()
            });
            gpu_metrics.engines = vec![
                taskmanager_application::GpuEngine {
                    name: "Render/3D".into(),
                    usage_pct: (gpu * 1.35).min(100.0),
                    ..Default::default()
                },
                taskmanager_application::GpuEngine {
                    name: "Copy".into(),
                    usage_pct: (gpu * 0.45).min(100.0),
                    ..Default::default()
                },
            ];
        }
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(Some(
                snapshot.clone(),
            ))),
        );

        let timestamp_ms = snapshot.timestamp_ms;
        let power_snapshot = capture_power_snapshot(timestamp_ms, 68 + index as u8 * 2, power);
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::PowerSupplies(Some(
                power_snapshot.clone(),
            )),
        );

        let sensors = capture_sensor_snapshot(timestamp_ms, 1_050 + index as u32 * 95, temperature);
        taskmanager_shell::fixture::seed_projection_fact(
            &mut app.shell,
            taskmanager_shell::fixture::ProjectionSeedFact::Sensors(Some(sensors.clone())),
        );
        taskmanager_shell::fixture::record_demo_history_frame(
            &mut app.shell,
            &snapshot,
            Some(&power_snapshot),
            Some(&sensors),
        );
    }
}

/// Optional targeted evidence fixture for the shared source-status surface.
/// This is reachable only through `demo_for_capture`, never through a normal
/// production launch; the rows remain present so the capture proves the
/// degraded-but-usable treatment rather than fabricating an empty page.
fn seed_capture_source_failure(app: &mut IcedApp) {
    let Some(name) = std::env::var_os("TM_ICED_CAPTURE_SOURCE_FAILURE")
        .and_then(|value| value.to_str().map(str::to_owned))
    else {
        return;
    };
    let Some(page) = capture_page_from_name(&name) else {
        return;
    };
    app.shell.application.active_page = page;
    let status = taskmanager_application::SourceStatus {
        provider: taskmanager_application::ProviderId::borrowed("capture.provider"),
        outcome: taskmanager_application::SourceOutcome::Unavailable(
            taskmanager_application::FailureKind::TimedOut,
        ),
        item_count: match page {
            taskmanager_application::AppPage::Services => {
                app.shell.projection().services.as_ref().map_or(0, Vec::len)
            }
            taskmanager_application::AppPage::Startup => app
                .shell
                .projection()
                .startup_entries
                .as_ref()
                .map_or(0, Vec::len),
            taskmanager_application::AppPage::Users => {
                app.shell.projection().sessions.as_ref().map_or(0, Vec::len)
            }
            _ => return,
        },
    };
    match page {
        taskmanager_application::AppPage::Services => {
            taskmanager_shell::fixture::seed_projection_fact(
                &mut app.shell,
                taskmanager_shell::fixture::ProjectionSeedFact::ServicesSource(Some(vec![status])),
            );
        }
        taskmanager_application::AppPage::Startup => {
            taskmanager_shell::fixture::seed_projection_fact(
                &mut app.shell,
                taskmanager_shell::fixture::ProjectionSeedFact::StartupSource(Some(vec![status])),
            );
        }
        taskmanager_application::AppPage::Users => {
            taskmanager_shell::fixture::seed_projection_fact(
                &mut app.shell,
                taskmanager_shell::fixture::ProjectionSeedFact::SessionsSource(Some(vec![status])),
            );
        }
        _ => {}
    }
}

fn capture_power_snapshot(
    timestamp_ms: u64,
    capacity_pct: u8,
    power_w: f32,
) -> taskmanager_application::PowerSupplySnapshot {
    let mut battery = taskmanager_application::BatteryInfo::new(
        "battery:demo:BAT0",
        taskmanager_application::DeviceState::healthy(timestamp_ms),
    );
    battery.status = "Discharging".into();
    battery.technology = "Li-ion".into();
    battery.model_name = "TaskForest Battery".into();
    battery.manufacturer = "TaskForest Lab".into();
    battery.apply_scalar_observations(taskmanager_application::BatteryScalarObservations {
        capacity_pct: taskmanager_application::ScalarObservation::available(
            capacity_pct,
            timestamp_ms,
        ),
        voltage_uv: taskmanager_application::ScalarObservation::available(12_480_000, timestamp_ms),
        power_w: taskmanager_application::ScalarObservation::available(power_w, timestamp_ms),
        cycle_count: taskmanager_application::ScalarObservation::available(184, timestamp_ms),
        // 49/56 Wh → 87.5% health; a discharge estimate consistent with the
        // "Discharging" status (the gated time_to_full stays absent).
        energy_full_uwh: taskmanager_application::ScalarObservation::available(
            49_000_000.0,
            timestamp_ms,
        ),
        energy_full_design_uwh: taskmanager_application::ScalarObservation::available(
            56_000_000.0,
            timestamp_ms,
        ),
        time_to_empty_secs: taskmanager_application::ScalarObservation::available(
            3_780.0,
            timestamp_ms,
        ),
        time_to_full_secs: taskmanager_application::ScalarObservation::unavailable(
            taskmanager_application::FailureKind::Unsupported,
        ),
    });
    taskmanager_application::PowerSupplySnapshot {
        state: taskmanager_application::DeviceState::healthy(timestamp_ms),
        timestamp_ms,
        batteries: vec![battery],
        ..Default::default()
    }
}

fn capture_sensor_snapshot(
    timestamp_ms: u64,
    rpm: u32,
    temperature_c: f32,
) -> taskmanager_application::SensorCenterSnapshot {
    let device_id = "hwmon:demo:cpu".to_string();
    taskmanager_application::SensorCenterSnapshot {
        state: taskmanager_application::DeviceState::healthy(timestamp_ms),
        timestamp_ms,
        readings: vec![
            capture_sensor_reading(
                device_id.clone().into(),
                "fan1",
                "CPU Fan",
                taskmanager_application::SensorDescriptor::fan_speed(
                    taskmanager_application::SensorScale::IDENTITY,
                ),
                taskmanager_application::SensorMagnitude::Unsigned(u64::from(rpm)),
                timestamp_ms,
            ),
            capture_sensor_reading(
                device_id.into(),
                "temp1",
                "Package",
                taskmanager_application::SensorDescriptor::temperature(
                    taskmanager_application::SensorScale::IDENTITY,
                ),
                taskmanager_application::SensorMagnitude::Decimal(f64::from(temperature_c)),
                timestamp_ms,
            ),
        ],
        ..Default::default()
    }
}

fn capture_sensor_reading(
    device_id: taskmanager_application::DeviceId,
    id: &str,
    label: &str,
    descriptor: taskmanager_application::SensorDescriptor,
    magnitude: taskmanager_application::SensorMagnitude,
    timestamp_ms: u64,
) -> taskmanager_application::SensorReading {
    let observation = taskmanager_application::SensorMeasurementObservation::available(
        descriptor.clone(),
        magnitude,
        timestamp_ms,
    )
    .unwrap_or_else(|_| {
        taskmanager_application::SensorMeasurementObservation::unavailable(
            descriptor,
            taskmanager_application::FailureKind::ProviderFault,
        )
    });
    taskmanager_application::SensorReading::from_measurement_observation(
        device_id,
        id.into(),
        label.into(),
        observation,
    )
}

fn capture_marker_path() -> Option<PathBuf> {
    std::env::var_os("TM_ICED_CAPTURE_MARKER_FILE").map(PathBuf::from)
}

/// Deterministic boot-evidence fixture for the demo frame: a measured
/// systemd-user critical chain (three timed units, one untimed node) so the
/// Startup-page waterfall renders in demo mode exactly like a measured boot.
/// This is honest fixture data, not a fabricated provider answer.
fn demo_boot_evidence() -> taskmanager_application::StartupBootEvidenceSnapshot {
    use taskmanager_application::{
        DeviceState, StartupBootEvidenceSnapshot, StartupCriticalChainNode,
    };
    let healthy = DeviceState::healthy(1_785_292_800_000);
    StartupBootEvidenceSnapshot {
        state: healthy,
        failed_units_state: healthy,
        critical_chain_state: healthy,
        failed_units_failure: None,
        critical_chain_failure: None,
        failed_units: Vec::new(),
        critical_chain: vec![
            StartupCriticalChainNode {
                unit: "dbus.service".into(),
                activated_at_ms: Some(500),
                duration_ms: Some(1_200),
            },
            StartupCriticalChainNode {
                unit: "network-online.target".into(),
                activated_at_ms: Some(1_700),
                duration_ms: Some(900),
            },
            StartupCriticalChainNode {
                unit: "graphical.target".into(),
                activated_at_ms: None,
                duration_ms: None,
            },
            StartupCriticalChainNode {
                unit: "multi-user.target".into(),
                activated_at_ms: Some(2_600),
                duration_ms: Some(2_500),
            },
        ],
    }
}

fn capture_device_from_name(name: &str) -> Option<PerfDevice> {
    match name {
        "cpu" => Some(PerfDevice::Cpu),
        "memory" => Some(PerfDevice::Memory),
        "disk" => Some(PerfDevice::Disk(0)),
        "network" => Some(PerfDevice::Network(0)),
        "gpu" => Some(PerfDevice::Gpu(0)),
        "battery" => Some(PerfDevice::Battery(0)),
        "fan" => Some(PerfDevice::Fan(0)),
        _ => None,
    }
}

fn capture_page_from_name(name: &str) -> Option<AppPage> {
    match name {
        "applications" => Some(AppPage::Applications),
        "services" => Some(AppPage::Services),
        "startup" => Some(AppPage::Startup),
        "users" => Some(AppPage::Users),
        "system" => Some(AppPage::System),
        "app-history" => Some(AppPage::AppHistory),
        _ => None,
    }
}

#[cfg(test)]
#[path = "../../tests/gui/app/constructors_tests.rs"]
mod tests;
