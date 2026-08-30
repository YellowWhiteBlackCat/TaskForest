use super::battery::battery_summary_lines;
use super::disk::partition_usage_text;
use super::gpu::gpu_headline_label_value;
use super::network::network_summary_lines;
use super::*;
use taskmanager_core::core::device_state::DeviceStatus;

/// Iced consumes the shared typed presentation keys; it does not maintain a
/// second failure-kind mapping.
#[test]
fn engine_rows_failure_keys_cover_every_typed_kind() {
    use taskmanager_platform_contract::CapabilityStatus;
    use taskmanager_shell::presentation::gpu_engine_rows::{
        GpuEngineRowsPresentation, present_gpu_engine_rows,
    };
    for (status, expected_key) in [
        (
            CapabilityStatus::Degraded(
                taskmanager_core::core::failure::FailureKind::PermissionDenied,
            ),
            "gpu.engines_permission_denied",
        ),
        (
            CapabilityStatus::MissingDependency,
            "gpu.engines_helper_unavailable",
        ),
        (CapabilityStatus::Unsupported, "gpu.engines_unsupported"),
        (
            CapabilityStatus::TemporarilyUnavailable,
            "gpu.engines_auth_unavailable",
        ),
    ] {
        let presentation = present_gpu_engine_rows(
            &taskmanager_application::GpuEngineRowsState::Closed,
            &taskmanager_core::core::identity::DeviceId::new("gpu:0"),
            Some(status),
        );
        assert_eq!(presentation.message_key(), Some(expected_key));
        assert!(!matches!(
            presentation,
            GpuEngineRowsPresentation::Active(_)
        ));
    }
}

#[test]
fn gpu_chart_layout_keeps_all_engines_standard_and_only_aggregate_compact() {
    use taskmanager_core::core::metrics::{GpuEngine, GpuEngineKind, GpuMetrics};

    let mut gpu = GpuMetrics::new("gpu0", "Fixture GPU");
    gpu.engines = vec![
        GpuEngine {
            name: "Render".into(),
            kind: GpuEngineKind::Render,
            usage_pct: 42.0,
        },
        GpuEngine {
            name: "Copy".into(),
            kind: GpuEngineKind::Copy,
            usage_pct: 7.0,
        },
        GpuEngine {
            name: String::new(),
            kind: GpuEngineKind::Unknown,
            usage_pct: f32::NAN,
        },
    ];
    // The layout derives from the typed chart inventory (both frame axes),
    // never from a local compact flag.
    let standard = projection::GpuChartLayout::for_inventory(
        crate::ui::responsive::PerformanceChartInventory::Full,
    );
    let compact = projection::GpuChartLayout::for_inventory(
        crate::ui::responsive::PerformanceChartInventory::AggregateOnly,
    );
    assert_eq!(standard, projection::GpuChartLayout::AggregateWithEngines);
    assert_eq!(compact, projection::GpuChartLayout::AggregateOnly);
    assert_eq!(
        standard
            .engine_charts(&gpu)
            .map(|engine| engine.name.as_str())
            .collect::<Vec<_>>(),
        ["Render", "Copy"]
    );
    assert_eq!(compact.engine_charts(&gpu).count(), 0);
    assert!(standard.shows_secondary_regions());
    assert!(!compact.shows_secondary_regions());
}

#[test]
fn compact_gpu_headline_projects_all_four_current_facts_without_a_selector() {
    use taskmanager_core::core::metrics::{GpuMetrics, GpuScalarObservations, ScalarObservation};

    taskmanager_test_support::pin_english();

    let mut gpu = GpuMetrics::new("gpu0", "Fixture GPU");
    gpu.apply_scalar_observations(GpuScalarObservations {
        utilization_pct: ScalarObservation::available(42.4, 1),
        temperature_c: ScalarObservation::available(57.6, 1),
        frequency_mhz: ScalarObservation::available(1_850, 1),
        power_w: ScalarObservation::unavailable(
            taskmanager_core::core::failure::FailureKind::Unsupported,
        ),
        ..GpuScalarObservations::default()
    });
    let metrics = projection::gpu_headline_metrics(&gpu);
    assert_eq!(
        metrics.map(|metric| metric.kind),
        [
            projection::GpuHeadlineKind::Utilization,
            projection::GpuHeadlineKind::Temperature,
            projection::GpuHeadlineKind::Frequency,
            projection::GpuHeadlineKind::Power,
        ]
    );
    assert_eq!(
        metrics
            .into_iter()
            .map(gpu_headline_label_value)
            .collect::<Vec<_>>(),
        [
            ("Utilization".to_string(), "42%".to_string()),
            ("Temperature".to_string(), "58 °C".to_string()),
            ("Clock".to_string(), "1850 MHz".to_string()),
            ("Power".to_string(), "—".to_string()),
        ]
    );
}

/// Look one summary row's value up by its localized label so the tests
/// assert the VALUE (the unit-pair formatting), not the surrounding row
/// order. A present-but-uncollected row reads as the shared dash.
fn row_value(rows: &[taskmanager_shell::viewmodel::StatRow], label: &str) -> String {
    rows.iter().find(|row| row.label() == label).map_or(
        "\u{ab}row absent\u{bb}".to_string(),
        |row| {
            row.value()
                .unwrap_or(taskmanager_shell::presentation::MISSING_VALUE)
                .to_string()
        },
    )
}

/// Battery health and runtime estimates are typed facts, not decorations:
/// the µWh pair derives 87.5% through the core rule, the estimate formats
/// through the shared duration formatter, and every unavailable fact leaves
/// its row entirely absent — never "0%" or "00h 00m".
#[test]
fn battery_health_and_estimate_rows_follow_typed_availability() {
    taskmanager_test_support::pin_english();
    use taskmanager_core::core::device_state::DeviceState;
    use taskmanager_core::core::metrics::ScalarObservation;
    use taskmanager_core::core::power::BatteryInfo;

    let mut battery = BatteryInfo::new("power-supply:BAT0", DeviceState::healthy(1));
    battery.apply_scalar_observations(taskmanager_core::core::power::BatteryScalarObservations {
        energy_full_uwh: ScalarObservation::available(49_000_000.0, 1),
        energy_full_design_uwh: ScalarObservation::available(56_000_000.0, 1),
        time_to_empty_secs: ScalarObservation::available(3_780.0, 1),
        ..Default::default()
    });
    let rows = battery_summary_lines(&battery);
    assert_eq!(row_value(&rows, t("battery.health")), "87.5%");
    assert_eq!(row_value(&rows, t("battery.time_to_empty")), "01h 03m");
    // The status-gated twin and any missing fact stay absent rows.
    assert_eq!(
        row_value(&rows, t("battery.time_to_full")),
        "\u{ab}row absent\u{bb}"
    );

    let sparse = battery_summary_lines(&BatteryInfo::new(
        "power-supply:BAT1",
        DeviceState::healthy(1),
    ));
    for key in [
        "battery.health",
        "battery.time_to_full",
        "battery.time_to_empty",
    ] {
        assert_eq!(
            row_value(&sparse, t(key)),
            "\u{ab}row absent\u{bb}",
            "{key} row must be absent when unavailable"
        );
    }
}

/// Static byte quantities follow the resolved Drive/Network unit pairs:
/// disk capacity/free, the per-partition used/total pair, and the NIC
/// cumulative totals print decimal bits under a bits+base-10 pair and
/// binary bytes under the default pair — the same matrix the rates
/// already honor (GPUI `disk_stats` / `partition_stats` /
/// `network_stats` parity). Before this they hardcoded base-2 bytes.
#[test]
fn static_quantities_follow_the_resolved_unit_pairs() {
    taskmanager_test_support::pin_english();
    const GIB: u64 = 1024 * 1024 * 1024;
    let mut disk = taskmanager_test_support::DiskMetricsFixtureBuilder::new()
        .mount_point("/".into())
        .current_capacity_bytes(2 * GIB)
        .current_available_bytes(GIB)
        .build();
    disk.partitions = vec![
        taskmanager_test_support::DiskPartitionFixtureBuilder::new()
            .mount_point("/".into())
            .name("nvme0n1p2".into())
            .scalar_observations(
                taskmanager_core::core::metrics::DiskPartitionScalarObservations {
                    capacity_bytes: taskmanager_core::core::metrics::ScalarObservation::available(
                        2 * GIB,
                        1,
                    ),
                    used_bytes: taskmanager_core::core::metrics::ScalarObservation::available(
                        GIB, 1,
                    ),
                    free_bytes: taskmanager_core::core::metrics::ScalarObservation::available(
                        GIB, 1,
                    ),
                },
            )
            .build(),
    ];
    // The partition census lives ONCE in the vital line (GPUI parity): the
    // stats rail carries the disk totals; per-partition usage renders in the
    // panel below the charts, never as duplicated stats rows.
    // Default pair (binary bytes) — the historical readout, unchanged.
    let rows = disk_summary_lines(&disk, true, true, &[]);
    assert_eq!(row_value(&rows, t("disk.capacity")), "2.0 GiB");
    assert_eq!(row_value(&rows, t("disk.free")), "1.0 GiB");
    let vital = disk_vital_line(&disk, crate::ui::UnitPrefs::default());
    assert!(vital.starts_with("1.0 GiB / 2.0 GiB"), "vital: {vital}");
    assert!(vital.ends_with("1 partitions"), "vital: {vital}");

    // Bits + base-10: the SAME quantities print decimal bits.
    let rows = disk_summary_lines(&disk, false, false, &[]);
    assert_eq!(row_value(&rows, t("disk.capacity")), "17.2 Gb");
    assert_eq!(row_value(&rows, t("disk.free")), "8.6 Gb");
    let vital = disk_vital_line(
        &disk,
        crate::ui::UnitPrefs {
            use_bytes: false,
            use_base2: false,
        },
    );
    assert!(vital.starts_with("8.6 Gb / 17.2 Gb"), "vital: {vital}");

    let nic = taskmanager_test_support::NetworkMetricsFixtureBuilder::new()
        .interface_name("enp3s0".into())
        .current_total_rx_bytes(1_500_000)
        .current_total_tx_bytes(750_000)
        .build();
    let rows = network_summary_lines(&nic, true, true);
    assert_eq!(row_value(&rows, t("net.total_received")), "1.4 MiB");
    assert_eq!(row_value(&rows, t("net.total_sent")), "732.4 KiB");
    // The network product default pair (bits, base-10).
    let rows = network_summary_lines(&nic, false, false);
    assert_eq!(row_value(&rows, t("net.total_received")), "12.0 Mb");
    assert_eq!(row_value(&rows, t("net.total_sent")), "6.0 Mb");
}

#[test]
fn partition_usage_does_not_turn_missing_free_space_into_zero() {
    // The label resolves through the shared catalog: pin the language so the
    // assertion is identical on any host locale (portability red line).
    taskmanager_application::i18n::set_language(taskmanager_application::i18n::Language::En);
    let (text, ratio) = partition_usage_text(
        Some(70),
        Some(100),
        None,
        DeviceStatus::Healthy,
        crate::ui::UnitPrefs::default(),
    );
    assert_eq!(ratio, None);
    assert_eq!(text, "Filesystem usage unavailable");
    assert!(!text.contains("0 B"));
}

/// The throughput graphs' scale carries the resolved unit pair, so the
/// caption summary and hover readout format through the SAME persisted
/// preference as the scalar rows — a NIC graph under the network product
/// default (bits, base-10) reads out in decimal bits, a disk graph under
/// the drive default in binary bytes.
#[test]
fn throughput_graph_scale_formats_readouts_through_the_resolved_pair() {
    let decimal_bits = UnitPrefs {
        use_bytes: false,
        use_base2: false,
    };
    assert_eq!(
        throughput_scale(decimal_bits),
        device_chart::DeviceMetricScale::BytesPerSecond {
            use_bytes: false,
            use_base2: false
        }
    );
    assert_eq!(
        device_chart::device_readout_text(throughput_scale(decimal_bits), &[0.0, 1_000_000.0], 1),
        Some("8.0 Mb/s".to_string()),
        "the hover pill follows the network pair, not hardcoded binary bytes"
    );
    assert_eq!(
        device_chart::device_readout_text(
            throughput_scale(UnitPrefs::default()),
            &[0.0, 1_048_576.0],
            1
        ),
        Some("1.0 MiB/s".to_string()),
        "the drive default pair keeps the historical binary-bytes readout"
    );
}

// ── GPU headline chart-metric selection (ADR-034 stage 2) ───────────────────

use taskmanager_shell::presentation::gpu_chart_metric::{
    GpuChartMetric, GpuChartMetricChoiceState,
};

/// One demo app parked on its GPU device: the shared selection's default and
/// its explicit per-family states read back through the same projection the
/// pill row renders.
fn gpu_selected_demo() -> crate::IcedApp {
    let mut app = crate::IcedApp::demo();
    let _ = app.update(Message::SelectPerfDevice(PerfDevice::Gpu(0)));
    app
}

fn choice_state(
    projection: &taskmanager_shell::presentation::gpu_chart_metric::GpuChartMetricProjection,
    metric: GpuChartMetric,
) -> GpuChartMetricChoiceState {
    projection
        .choices
        .iter()
        .find(|choice| choice.metric == metric)
        .map(|choice| choice.state)
        .expect("every vocabulary family is projected")
}

/// The demo GPU observes utilization/temperature/frequency/idle-residency
/// but no power draw: the shared projection defaults to Utilization, keeps
/// the unobserved family present-and-explicit, and marks every observed
/// family selectable (ADR-034: 不隐藏、不伪造).
#[test]
fn gpu_chart_metric_projection_defaults_to_utilization_and_gates_families() {
    let app = gpu_selected_demo();
    let gpu = app.viewed_gpu().expect("demo views its GPU");
    let gate = taskmanager_shell::gpu_chart_metric_gate(Some(gpu));
    let projection = app.shell.gpu_chart_metric_projection(&gate);

    assert_eq!(projection.selected, GpuChartMetric::Utilization);
    assert_eq!(
        choice_state(&projection, GpuChartMetric::Utilization),
        GpuChartMetricChoiceState::Selected
    );
    assert_eq!(
        choice_state(&projection, GpuChartMetric::Power),
        GpuChartMetricChoiceState::Unavailable,
        "the unobserved power family must stay present and explicit"
    );
    assert_eq!(
        choice_state(&projection, GpuChartMetric::Temperature),
        GpuChartMetricChoiceState::Selectable
    );
    assert_eq!(projection.choices.len(), GpuChartMetric::ALL.len());
}

/// The shell's availability gate (ADR-034) accepts an observed family and
/// silently rejects an unobserved one; the family windows follow the
/// selection — the unobserved family's window stays honest gaps, never a
/// fabricated zero. Driven through the shell seam directly: the Iced surface
/// renders every measured family simultaneously (engine_graph "no metric
/// selector" parity), so no Iced message carries a selection.
#[test]
fn gpu_chart_metric_selects_through_the_shared_gate() {
    let mut app = gpu_selected_demo();
    let viewed = app.viewed_gpu().expect("demo views its GPU").clone();
    let device_id = viewed.device_id.clone();
    let device_generation = viewed.device_generation.get();
    let gate = taskmanager_shell::gpu_chart_metric_gate(Some(&viewed));

    app.shell
        .select_gpu_chart_metric(GpuChartMetric::Power, &gate);
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "an unavailable family is rejected with no state change"
    );

    app.shell
        .select_gpu_chart_metric(GpuChartMetric::Temperature, &gate);
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature
    );

    let temperature = app.cached_gpu_chart_metric_series(
        &device_id,
        device_generation,
        GpuChartMetric::Temperature,
    );
    assert!(
        temperature.iter().any(|value| value.is_finite()),
        "the selected family's window carries its real samples"
    );
    let power =
        app.cached_gpu_chart_metric_series(&device_id, device_generation, GpuChartMetric::Power);
    assert!(
        power.iter().all(|value| !value.is_finite()),
        "the unobserved family's window stays explicit gaps"
    );
}

/// When even the default family is unobserved, the selection stays on it and
/// projects `SelectedUnavailable` — the pill row keeps the family visible and
/// inert, and the chart renders the collecting/gap state instead of a
/// fabricated zero.
#[test]
fn gpu_chart_metric_selected_unavailable_stays_explicit() {
    let mut app = gpu_selected_demo();
    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        if let Some(snapshot) = snapshot.as_mut()
            && let Some(gpu) = snapshot.gpu.first_mut()
        {
            gpu.apply_scalar_observations(taskmanager_core::core::metrics::GpuScalarObservations {
                utilization_pct: taskmanager_core::core::metrics::ScalarObservation::unavailable(
                    taskmanager_core::core::failure::FailureKind::Unsupported,
                ),
                ..taskmanager_core::core::metrics::GpuScalarObservations::default()
            });
        }
    });
    let gpu = app.viewed_gpu().expect("demo still views its GPU");
    let gate = taskmanager_shell::gpu_chart_metric_gate(Some(gpu));
    app.shell.reconcile_gpu_chart_metric(&gate);
    let projection = app.shell.gpu_chart_metric_projection(&gate);

    assert_eq!(projection.selected, GpuChartMetric::Utilization);
    assert_eq!(
        choice_state(&projection, GpuChartMetric::Utilization),
        GpuChartMetricChoiceState::SelectedUnavailable,
        "the honest degradation keeps the family selected-and-explicit"
    );
}

/// The live per-tick fold (`finish_tick_system`, ADR-034 stage 2): every
/// tick reconciles the shared selection against the viewed device's fresh
/// facts, so a generation advance — a confirmed hot-plug — resets the
/// selection to the Utilization default in the frame that carried the fact.
/// The selection itself is driven through the shell seam: the Iced surface
/// renders every measured family simultaneously (the no-selector parity
/// note in `engine_graph`), so no Iced message carries a selection.
#[test]
fn gpu_chart_metric_generation_change_resets_to_the_default() {
    use taskmanager_core::core::identity::DeviceGeneration;

    let mut app = gpu_selected_demo();
    let gate = taskmanager_shell::gpu_chart_metric_gate(app.viewed_gpu());
    app.shell
        .select_gpu_chart_metric(GpuChartMetric::Temperature, &gate);
    let _ = app.update(Message::Tick);
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Temperature,
        "a stable generation keeps the user's selection"
    );

    taskmanager_shell::fixture::edit_snapshot(&mut app.shell, |snapshot| {
        if let Some(snapshot) = snapshot.as_mut()
            && let Some(gpu) = snapshot.gpu.first_mut()
        {
            gpu.device_generation =
                DeviceGeneration::new(gpu.device_generation.get().saturating_add(1));
        }
    });
    let _ = app.update(Message::Tick);
    assert_eq!(
        app.shell.gpu_chart_metric_selected(),
        GpuChartMetric::Utilization,
        "a generation change resets the selection to the ADR default"
    );
}
