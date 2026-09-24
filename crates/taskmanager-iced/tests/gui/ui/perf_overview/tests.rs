use super::*;
use taskmanager_application::{
    CorrelatedEvent, MsrReadoutEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_core::core::metrics::CpuPackageMetrics;
use taskmanager_core::core::metrics::MemoryMetrics;
use taskmanager_core::core::metrics::PressureWindow;
use taskmanager_core::core::metrics::ResourcePressure;
use taskmanager_core::core::metrics::{MsrReadoutSnapshot, MsrThermalStatusReadout};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};
use taskmanager_shell::fixture::edit_snapshot;
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::presentation::cpu_thermal_throttle_summary;
use taskmanager_shell::presentation::msr_thermal_status_summary;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_test_support::pin_english;

mod graph_summary_tests {
    use super::*;

    #[test]
    fn graph_summary_line_keeps_latest_average_and_peak_honest() {
        let mut lines = Vec::new();
        push_graph_summary(&mut lines, "CPU", &[10.0, f32::NAN, 30.0], |value| {
            format!("{value:.0}%")
        });
        assert_eq!(lines.len(), 1, "finite samples should produce one line");
    }

    #[test]
    fn graph_summary_line_drops_an_all_gap_window() {
        let mut lines = Vec::new();
        push_graph_summary(&mut lines, "CPU", &[f32::NAN], |value| {
            format!("{value:.0}%")
        });
        assert!(
            lines.is_empty(),
            "all-gap history must not render fake stats"
        );
    }

    #[test]
    fn headline_chart_floor_keeps_a_legible_but_bounded_viewport() {
        // GPUI headline-tier parity, raised to GPUI's actual ~276px presence
        // at a 780px window (ICED-024-7): the floor keeps the chart legible
        // while the fixed-viewport frames still bound it by the column.
        assert_eq!(cpu::HEADLINE_CHART_FLOOR, 240.0);
    }
}

mod memory_stats_tests {
    use super::*;
    use taskmanager_core::core::failure::FailureKind;
    use taskmanager_core::core::metrics::{OptionalObservation, ScalarObservation};

    use taskmanager_test_support::MemoryMetricsFixtureBuilder;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    fn flat(stats: &[StatRow]) -> Vec<(&str, &str)> {
        stats
            .iter()
            .map(|row| (row.label(), row.value().unwrap_or(MISSING_VALUE)))
            .collect()
    }

    fn rich_memory() -> MemoryMetrics {
        MemoryMetricsFixtureBuilder::new()
            .current_total_bytes(16 * GIB)
            .current_used_bytes(8 * GIB)
            .current_available_bytes(4 * GIB)
            .current_swap_total_bytes(8 * GIB)
            .current_swap_used_bytes(GIB)
            .current_used_rate_mib_per_sec(1.5)
            .cached_bytes(2 * GIB)
            .buffers_bytes(512 * MIB)
            .hardware_reserved_bytes(256 * MIB)
            .speed_mhz(5_600)
            .slots_used(2)
            .slots_total(4)
            .committed_bytes(10 * GIB)
            .commit_limit_bytes(16 * GIB)
            .compressed_swap_used_bytes(512 * MIB)
            .compressed_swap_capacity_bytes(4 * GIB)
            .compressed_swap_memory_used_bytes(GIB)
            .compressed_swap_cache_enabled(true)
            .build()
    }

    #[test]
    fn memory_rows_match_the_gpui_row_set_and_format() {
        pin_english();
        assert_eq!(
            flat(&memory_stats_rows(&rich_memory(), true, true)),
            vec![
                ("In use", "8.0 GiB"),
                ("Available", "4.0 GiB"),
                ("Hardware reserved", "256.0 MiB"),
                ("Cached", "2.0 GiB"),
                ("Buffers", "512.0 MiB"),
                ("Swap", "1.0 GiB / 8.0 GiB"),
                ("Speed", "5600 MT/s"),
                ("Slots", "2 / 4"),
                ("Committed", "10.0 GiB / 16.0 GiB"),
                ("zram swap", "512.0 MiB / 4.0 GiB"),
                ("zram RAM used", "1.0 GiB"),
                ("zswap", "Enabled"),
                ("Usage rate", "+1.5 MiB/s"),
            ]
        );
    }

    #[test]
    fn empty_metrics_keep_base_rows_honest_and_drop_gated_rows() {
        pin_english();
        assert_eq!(
            flat(&memory_stats_rows(&MemoryMetrics::default(), true, true)),
            vec![
                ("In use", "—"),
                ("Available", "—"),
                ("Hardware reserved", "—"),
                ("Cached", "—"),
                ("Swap", "—"),
                ("Speed", "—"),
                ("Slots", "—"),
            ]
        );
    }

    #[test]
    fn measured_zero_is_a_value_but_near_zero_rate_is_suppressed() {
        pin_english();
        let mut memory = MemoryMetricsFixtureBuilder::new()
            .current_total_bytes(8 * GIB)
            .current_used_rate_mib_per_sec(0.0)
            .compressed_swap_used_bytes(0)
            .compressed_swap_capacity_bytes(2 * GIB)
            .compressed_swap_memory_used_bytes(0)
            .compressed_swap_cache_enabled(false)
            .build();
        let stats = memory_stats_rows(&memory, true, true);
        let rows = flat(&stats);
        assert!(rows.contains(&("zram swap", "0 B / 2.0 GiB")));
        // A measured zero RAM cost is a real value, not a missing row.
        assert!(rows.contains(&("zram RAM used", "0 B")));
        assert!(rows.contains(&("zswap", "Disabled")));
        assert!(
            !rows.iter().any(|(label, _)| *label == "Usage rate"),
            "an idle 0.0 MiB/s must not render a noisy row"
        );

        let mut scalar = *memory.scalar_observations();
        scalar.used_rate_mib_per_sec = ScalarObservation::available(0.01, 2);
        memory.apply_observations(scalar, memory.optional_observations().clone());
        assert!(
            !flat(&memory_stats_rows(&memory, true, true))
                .iter()
                .any(|(label, _)| *label == "Usage rate"),
            "|0.01| < 0.05 stays suppressed"
        );

        let mut scalar = *memory.scalar_observations();
        scalar.used_rate_mib_per_sec = ScalarObservation::available(-0.5, 3);
        memory.apply_observations(scalar, memory.optional_observations().clone());
        assert!(
            flat(&memory_stats_rows(&memory, true, true)).contains(&("Usage rate", "−512.0 KiB/s")),
            "a freeing rate keeps the signed minus"
        );
    }

    #[test]
    fn failed_commit_observation_hides_the_committed_pair() {
        pin_english();
        let mut memory = rich_memory();
        let mut optional = memory.optional_observations().clone();
        optional.virtual_memory_commit.committed_bytes =
            OptionalObservation::unavailable(FailureKind::TimedOut);
        memory.apply_observations(*memory.scalar_observations(), optional);
        let stats = memory_stats_rows(&memory, true, true);
        let rows = flat(&stats);
        assert!(
            !rows.iter().any(|(label, _)| *label == "Committed"),
            "an unavailable commit observation must not render bytes"
        );
        assert!(
            rows.contains(&("zram swap", "512.0 MiB / 4.0 GiB")),
            "untouched families keep their rows"
        );
    }

    #[test]
    fn swap_throughput_rows_render_the_observed_system_rates() {
        pin_english();
        const KIB: u64 = 1024;

        // The kernel swap counters are a two-sample rate, so the fixture
        // installs the observed per-second reads through the same canonical
        // scalar group the provider applies.
        let mut memory = MemoryMetricsFixtureBuilder::new()
            .current_total_bytes(16 * GIB)
            .current_swap_total_bytes(8 * GIB)
            .current_swap_used_bytes(GIB)
            .build();
        let mut scalar = *memory.scalar_observations();
        scalar.swap_in_bytes_per_sec = ScalarObservation::available(2 * MIB, 2);
        scalar.swap_out_bytes_per_sec = ScalarObservation::available(512 * KIB, 2);
        memory.apply_observations(scalar, memory.optional_observations().clone());

        let memory_rows = memory_stats_rows(&memory, true, true);
        let rows = flat(&memory_rows);
        assert!(
            rows.contains(&("Swap in", "2.0 MiB/s")),
            "the observed swap-in rate must render in the shared row set: {rows:?}"
        );
        assert!(
            rows.contains(&("Swap out", "512.0 KiB/s")),
            "the observed swap-out rate must render in the shared row set: {rows:?}"
        );

        // A first sample has no rate yet: both rows stay absent instead of a
        // fabricated 0 B/s, so a cold window reads as unobserved.
        let cold_rows_source = memory_stats_rows(&MemoryMetrics::default(), true, true);
        let cold_rows = flat(&cold_rows_source);
        assert!(
            !cold_rows
                .iter()
                .any(|(label, _)| *label == "Swap in" || *label == "Swap out"),
            "an unobserved swap rate must not become a zero row: {cold_rows:?}"
        );
    }

    #[test]
    fn signed_rate_respects_the_unit_preferences() {
        pin_english();
        assert_eq!(
            stats::signed_memory_rate_text(1.5, true, true),
            "+1.5 MiB/s"
        );
        assert_eq!(
            stats::signed_memory_rate_text(-0.5, true, true),
            "−512.0 KiB/s"
        );
        assert_eq!(
            stats::signed_memory_rate_text(1.5, false, true),
            "+12.0 Mib/s"
        );
    }
}

mod cpu_frequency_source_tests {
    use super::*;

    #[test]
    fn speed_row_relabels_bogomips_and_never_fakes_a_mhz_clock() {
        pin_english();
        // The row keeps its typed missingness: an absent frequency is `None`
        // (the shared dash), never a fabricated clock.
        assert_eq!(cpu_speed_row(Some(5300), true).label(), "BogoMIPS");
        assert_eq!(
            cpu_speed_row(Some(5300), true).value(),
            Some("5300.00 BogoMIPS")
        );
        assert_eq!(cpu_speed_row(None, true).value(), None);
        assert_eq!(cpu_speed_row(Some(3500), false).label(), "Speed");
        assert_eq!(cpu_speed_row(Some(3500), false).value(), Some("3500 MHz"));
        assert_eq!(cpu_speed_row(None, false).value(), None);
    }

    #[test]
    fn cpu_headline_readouts_format_every_current_metric_without_graph_selection() {
        pin_english();
        let metrics = projection::cpu_headline_metrics(Some(projection::CpuObservation {
            usage_pct: Some(37.0),
            frequency_mhz: Some(3_500),
            temperature_c: Some(54.0),
            power_w: Some(18.2),
            pressure: Some(ResourcePressure::some_only(PressureWindow::new(
                0.5, 0.4, 0.3, 0,
            ))),
        }));
        assert_eq!(
            metrics
                .into_iter()
                .map(|metric| cpu_headline_label_value(
                    metric,
                    false,
                    CpuTemperatureSource::Coretemp
                ))
                .collect::<Vec<_>>(),
            vec![
                ("Utilization".to_string(), "37%".to_string()),
                ("Speed".to_string(), "3500 MHz".to_string()),
                ("Temperature".to_string(), "54 °C".to_string()),
                ("Power".to_string(), "18.2 W".to_string()),
                (
                    "Stall".to_string(),
                    "some 10s 0.5% · 60s 0.4% · 5m 0.3%".to_string(),
                ),
            ]
        );
        assert_eq!(
            cpu_headline_label_value(metrics[1], true, CpuTemperatureSource::Coretemp),
            ("BogoMIPS".to_string(), "3500.00 BogoMIPS".to_string())
        );
        // Labeled fallback tiers qualify the temperature value so a derived
        // reading never masquerades as a dedicated CPU sensor chip.
        assert_eq!(
            cpu_headline_label_value(metrics[2], false, CpuTemperatureSource::PackageHwmon),
            (
                "Temperature".to_string(),
                "54 °C · hwmon fallback".to_string()
            )
        );
        assert_eq!(
            cpu_headline_label_value(metrics[2], false, CpuTemperatureSource::ThermalZone),
            (
                "Temperature".to_string(),
                "54 °C · ACPI thermal zone".to_string()
            )
        );
    }

    #[test]
    fn cpu_headline_projection_uses_canonical_order_and_keeps_gaps_explicit() {
        let projected = projection::cpu_headline_metrics(Some(projection::CpuObservation {
            usage_pct: Some(42.0),
            frequency_mhz: Some(2_400),
            temperature_c: None,
            power_w: None,
            pressure: None,
        }));
        assert_eq!(
            projected.map(|metric| metric.kind),
            [
                projection::CpuHeadlineKind::Utilization,
                projection::CpuHeadlineKind::Frequency,
                projection::CpuHeadlineKind::Temperature,
                projection::CpuHeadlineKind::Power,
                projection::CpuHeadlineKind::Pressure,
            ],
            "headline readouts must keep the fixed Iced presentation order"
        );
        assert_eq!(projected[2].value, None);
        assert_eq!(projected[3].value, None);
        assert_eq!(
            cpu_headline_label_value(projected[2], false, CpuTemperatureSource::Coretemp).1,
            "—",
            "an unavailable current observation must not become zero"
        );
        assert_eq!(
            cpu_headline_label_value(projected[2], false, CpuTemperatureSource::PackageHwmon).1,
            "—",
            "a missing temperature never grows a source qualifier"
        );
    }

    #[test]
    fn cpu_chart_layout_keeps_secondary_surfaces_out_of_compact_space() {
        assert_eq!(
            projection::CpuChartLayout::for_inventory(
                crate::ui::responsive::PerformanceChartInventory::Full
            ),
            projection::CpuChartLayout::AggregateWithPerCore
        );
        assert_eq!(
            projection::CpuChartLayout::for_inventory(
                crate::ui::responsive::PerformanceChartInventory::AggregateOnly
            ),
            projection::CpuChartLayout::AggregateOnly
        );
    }
}

mod cpu_throttle_tests {
    use super::*;

    fn set_package_counters(
        app: &mut crate::IcedApp,
        counters: &[(u32, Option<u64>, Option<u64>)],
    ) {
        edit_snapshot(&mut app.shell, |snapshot| {
            let snapshot = snapshot.as_mut().expect("demo snapshot");
            snapshot.cpu.packages = counters
                .iter()
                .map(
                    |&(package_id, package_throttle_count, core_throttle_count)| {
                        let mut package = CpuPackageMetrics::new(package_id);
                        package.package_throttle_count = package_throttle_count;
                        package.core_throttle_count = core_throttle_count;
                        package
                    },
                )
                .collect();
        });
    }

    fn throttle_row(stats: &[StatRow]) -> Option<&StatRow> {
        stats
            .iter()
            .find(|row| row.label() == t("cpu.thermal_throttle"))
    }

    /// The `power.thermal-throttle-events` delivery: the Performance CPU stat
    /// column renders the shared
    /// [`taskmanager_shell::presentation::cpu_thermal_throttle_summary`] fold
    /// (whose exact value form is pinned once by the shell rule test) and omits
    /// the row entirely when no package observed a counter — never a fabricated
    /// zero.
    #[test]
    fn cpu_stats_render_the_thermal_throttle_counters_with_honest_absence() {
        pin_english();
        let mut app = crate::IcedApp::demo();

        set_package_counters(&mut app, &[(0, Some(7), Some(3)), (1, Some(12), None)]);
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        let row = throttle_row(&stats).expect("observed counters must grow a stat row");
        let expected = {
            let snapshot = app
                .shell
                .projection()
                .snapshot
                .as_ref()
                .expect("demo snapshot");
            cpu_thermal_throttle_summary(&snapshot.cpu)
                .expect("observed counters must produce the shared fold")
        };
        assert_eq!(
            row.value(),
            Some(expected.as_str()),
            "the stat row must render the shared thermal-throttle fold"
        );

        set_package_counters(&mut app, &[(0, None, None)]);
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        assert!(
            throttle_row(&stats).is_none(),
            "an unobserved counter family must omit its row, never fabricate a zero"
        );
    }

    /// Seed the shared MSR session with an accepted readout carrying the given
    /// real-time `IA32_THERM_STATUS` bits, through the same request-session and
    /// platform-batch path production admission uses.
    fn set_msr_thermal_status(app: &mut crate::IcedApp, bits: &[(u32, Option<bool>)]) {
        let attempt = app.shell.begin_msr_readout_request();
        let request_id = RequestId::new(41).expect("fixture request id");
        assert!(app.shell.accept_msr_readout_request(attempt, request_id));
        let mut batch = PlatformEventBatch::default();
        batch.msr_readout_events.push(CorrelatedEvent::new(
            PlatformEventContext {
                request_id,
                capability: CapabilityId::TELEMETRY_CPU_MSR,
                provider: None,
                sequence: EventSequence::new(1),
                observed_at_ms: 10,
            },
            MsrReadoutEvent::Update(
                MsrReadoutSnapshot::success(Vec::new()).with_thermal(
                    bits.iter()
                        .map(|&(cpu, thermal_status)| MsrThermalStatusReadout {
                            cpu,
                            thermal_status,
                            ..MsrThermalStatusReadout::default()
                        })
                        .collect(),
                ),
            ),
        ));
        app.shell.apply_platform_batch(batch);
    }

    fn thermal_status_row(stats: &[StatRow]) -> Option<&StatRow> {
        stats
            .iter()
            .find(|row| row.label() == t("cpu.thermal_status"))
    }

    /// The real-time thermal-status / PROCHOT delivery: the Performance CPU
    /// stat column renders the shared
    /// [`taskmanager_shell::presentation::msr_thermal_status_summary`] fold
    /// beside the cumulative counters — the shared asserted/clear words and
    /// the honest dash for an unreadable register — and omits the whole row
    /// while the privileged lane produced no accepted readout.
    #[test]
    fn cpu_stats_render_the_real_time_thermal_status_with_honest_absence() {
        pin_english();
        let mut app = crate::IcedApp::demo();
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        assert!(
            thermal_status_row(&stats).is_none(),
            "a session that never ran must not grow a real-time row"
        );

        set_msr_thermal_status(&mut app, &[(0, Some(true)), (1, Some(false)), (2, None)]);
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        let row = thermal_status_row(&stats).expect("an accepted readout must grow a stat row");
        let expected = msr_thermal_status_summary(app.shell.msr_readout_state())
            .expect("an accepted readout must fold");
        assert_eq!(
            row.value(),
            Some(expected.as_str()),
            "the stat row must render the shared real-time status fold"
        );
        assert!(
            expected.contains(t("cpu.thermal_status_asserted"))
                && expected.contains(t("cpu.thermal_status_clear")),
            "the fold must carry the shared asserted/clear vocabulary: {expected}"
        );
        assert!(
            expected.contains(MISSING_VALUE),
            "an unreadable register must keep the honest dash: {expected}"
        );
    }
}
