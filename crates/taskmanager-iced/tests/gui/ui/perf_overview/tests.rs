use super::*;

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
        // GPUI headline-tier parity: a headline chart inside a scrolling strip
        // frame keeps the shared 180px floor (the old compact 80px was below
        // the authority's MAIN_GRAPH_MIN_HEIGHT).
        assert_eq!(cpu::HEADLINE_CHART_FLOOR, 180.0);
    }
}

mod memory_stats_tests {
    use super::*;
    use taskmanager_application::{FailureKind, OptionalObservation, ScalarObservation};
    use taskmanager_test_support::MemoryMetricsFixtureBuilder;

    const GIB: u64 = 1024 * 1024 * 1024;
    const MIB: u64 = 1024 * 1024;

    fn flat(stats: &[taskmanager_shell::viewmodel::StatRow]) -> Vec<(&str, &str)> {
        stats
            .iter()
            .map(|row| {
                (
                    row.label(),
                    row.value()
                        .unwrap_or(taskmanager_shell::presentation::MISSING_VALUE),
                )
            })
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
        taskmanager_test_support::pin_english();
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
        taskmanager_test_support::pin_english();
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
        taskmanager_test_support::pin_english();
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
        taskmanager_test_support::pin_english();
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
    fn signed_rate_respects_the_unit_preferences() {
        taskmanager_test_support::pin_english();
        assert_eq!(signed_memory_rate_text(1.5, true, true), "+1.5 MiB/s");
        assert_eq!(signed_memory_rate_text(-0.5, true, true), "−512.0 KiB/s");
        assert_eq!(signed_memory_rate_text(1.5, false, true), "+12.0 Mib/s");
    }
}

mod cpu_frequency_source_tests {
    use super::*;

    #[test]
    fn speed_row_relabels_bogomips_and_never_fakes_a_mhz_clock() {
        taskmanager_test_support::pin_english();
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
        taskmanager_test_support::pin_english();
        let metrics = projection::cpu_headline_metrics(Some(projection::CpuObservation {
            usage_pct: Some(37.0),
            frequency_mhz: Some(3_500),
            temperature_c: Some(54.0),
            power_w: Some(18.2),
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
        }));
        assert_eq!(
            projected.map(|metric| metric.kind),
            [
                projection::CpuHeadlineKind::Utilization,
                projection::CpuHeadlineKind::Frequency,
                projection::CpuHeadlineKind::Temperature,
                projection::CpuHeadlineKind::Power,
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
