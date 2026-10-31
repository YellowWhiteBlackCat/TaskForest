use super::*;
use crate::app::{Message, PerfDevice};
use crate::ui::core_grid::CORE_GRID_SCROLL_ID;
use crate::ui::perf_layout::PERFORMANCE_STATS_RAIL_SCROLL_ID;
use crate::ui::responsive::{
    PerformanceChartInventory, PerformancePageBudget, PerformanceVerticalRunway,
};
use iced::advanced::layout::{Layout, Limits};
use iced::advanced::renderer::Headless;
use iced::advanced::widget::operation::Scrollable;
use iced::advanced::widget::operation::scrollable::{AbsoluteOffset, scroll_to};
use iced::advanced::widget::{Id, Operation, Tree};
use iced::{Rectangle, Size, Vector};
use taskforest_contract::{CapabilityId, EventSequence, RequestId};
use taskforest_core::core::hardware::CoreBreakdown;
use taskforest_core::core::metrics::CpuPackageMetrics;
use taskforest_core::core::metrics::MemoryMetrics;
use taskforest_core::core::metrics::PressureWindow;
use taskforest_core::core::metrics::ResourcePressure;
use taskforest_core::core::metrics::{MsrReadoutSnapshot, MsrThermalStatusReadout};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    AppPage, CorrelatedEvent, MsrReadoutEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_assets::embedded_fonts;
use taskmanager_shell::fixture::{edit_hardware, edit_snapshot};
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::presentation::cpu_thermal_throttle_summary;
use taskmanager_shell::presentation::msr_thermal_status_segments;
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
    use taskforest_core::core::failure::FailureKind;
    use taskforest_core::core::metrics::{OptionalObservation, ScalarObservation};

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
    use crate::app::FocusTarget;
    use crate::focus::focus_id;
    use iced::advanced::widget::operation::Focusable;
    use taskforest_core::core::failure::FailureKind;

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

    #[derive(Debug, Default)]
    struct RenderedTextBounds {
        fragments: Vec<(String, Rectangle)>,
        focusable_bounds: Vec<(Id, Rectangle)>,
        scrollable_regions: Vec<(Rectangle, Rectangle)>,
        core_grid_viewport: Option<Rectangle>,
        core_grid_content: Option<Rectangle>,
        core_grid_translation: Option<Vector>,
        stats_rail: Option<(Rectangle, Rectangle, Vector)>,
    }

    impl Operation for RenderedTextBounds {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }

        fn text(&mut self, _id: Option<&Id>, bounds: Rectangle, fragment: &str) {
            self.fragments.push((fragment.to_owned(), bounds));
        }

        fn focusable(&mut self, id: Option<&Id>, bounds: Rectangle, _state: &mut dyn Focusable) {
            if let Some(id) = id {
                self.focusable_bounds.push((id.clone(), bounds));
            }
        }

        fn scrollable(
            &mut self,
            id: Option<&Id>,
            bounds: Rectangle,
            content_bounds: Rectangle,
            translation: Vector,
            _state: &mut dyn Scrollable,
        ) {
            self.scrollable_regions.push((bounds, content_bounds));
            if id.is_some_and(|id| id == &Id::new(CORE_GRID_SCROLL_ID)) {
                self.core_grid_viewport = Some(bounds);
                self.core_grid_content = Some(content_bounds);
                self.core_grid_translation = Some(translation);
            }
            if id.is_some_and(|id| id == &Id::new(PERFORMANCE_STATS_RAIL_SCROLL_ID)) {
                self.stats_rail = Some((bounds, content_bounds, translation));
            }
        }
    }

    fn core_label_bounds(rendered: &RenderedTextBounds, label_prefix: &str) -> Option<Rectangle> {
        let translation = rendered.core_grid_translation?;
        rendered.fragments.iter().find_map(|(fragment, bounds)| {
            fragment.starts_with(label_prefix).then_some(Rectangle {
                x: bounds.x - translation.x,
                y: bounds.y - translation.y,
                ..*bounds
            })
        })
    }

    fn core_cell_text_bounds(
        rendered: &RenderedTextBounds,
        label_prefix: &str,
    ) -> Option<(Rectangle, Rectangle)> {
        let translation = rendered.core_grid_translation?;
        let label_index = rendered
            .fragments
            .iter()
            .position(|(fragment, _)| fragment.starts_with(label_prefix))?;
        let content_bounds = |bounds: Rectangle| Rectangle {
            x: bounds.x - translation.x,
            y: bounds.y - translation.y,
            ..bounds
        };
        let label = content_bounds(rendered.fragments[label_index].1);
        let readout = rendered.fragments[label_index + 1..]
            .iter()
            .take_while(|(fragment, _)| !fragment.starts_with('C'))
            .find(|(fragment, _)| !fragment.trim().is_empty())?
            .1;
        Some((label, content_bounds(readout)))
    }

    fn software_renderer() -> iced::Renderer {
        {
            let mut fonts = iced::advanced::graphics::text::font_system()
                .write()
                .expect("font system");
            for font in embedded_fonts() {
                fonts.load_font(font);
            }
        }
        iced::futures::executor::block_on(iced::Renderer::new(
            crate::theme_binding::BUNDLED_UI_FONT,
            iced::Pixels(16.0),
            Some("tiny-skia"),
        ))
        .expect("software renderer")
    }

    fn rendered_text_bounds(
        app: &crate::IcedApp,
        size: Size,
        renderer: &iced::Renderer,
        scroll_offset: Option<f32>,
    ) -> RenderedTextBounds {
        rendered_text_bounds_for_scroll(
            app,
            size,
            renderer,
            scroll_offset.map(|offset| (CORE_GRID_SCROLL_ID, offset)),
        )
    }

    fn rendered_text_bounds_for_scroll(
        app: &crate::IcedApp,
        size: Size,
        renderer: &iced::Renderer,
        scroll: Option<(&'static str, f32)>,
    ) -> RenderedTextBounds {
        let mut view = crate::ui::view(app);
        let mut tree = Tree::new(&view);
        let mut node =
            view.as_widget_mut()
                .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));
        if let Some((scroll_id, offset_y)) = scroll {
            let mut operation = scroll_to(
                Id::new(scroll_id),
                AbsoluteOffset {
                    x: None,
                    y: Some(offset_y),
                },
            );
            view.as_widget_mut()
                .operate(&mut tree, Layout::new(&node), renderer, &mut operation);
            node = view
                .as_widget_mut()
                .layout(&mut tree, renderer, &Limits::new(Size::ZERO, size));
        }
        let mut operation = RenderedTextBounds::default();
        view.as_widget_mut()
            .operate(&mut tree, Layout::new(&node), renderer, &mut operation);
        operation
    }

    #[test]
    fn cpu_per_core_rows_are_reachable_at_standard_size_and_typed_away_in_compact() {
        pin_english();
        let mut app = crate::IcedApp::demo_for_capture_with(None, None);
        let _ = app.update(Message::SelectPage(AppPage::Performance));
        let _ = app.update(Message::SelectPerfDevice(PerfDevice::Cpu));
        let renderer = software_renderer();

        let standard_size = Size::new(1180.0, 780.0);
        let _ = app.update(Message::WindowResized(standard_size));
        let standard_budget = PerformancePageBudget::for_perf_frame(
            app.page_viewport_size(),
            app.performance.sidebar_visible,
        );
        assert_eq!(standard_budget.vertical, PerformanceVerticalRunway::Charts);
        assert_eq!(
            standard_budget.chart_inventory,
            PerformanceChartInventory::Full
        );
        let per_core = app.cached_per_core_usage_series();
        let expected_core_count = app
            .shell
            .projection()
            .hardware
            .as_ref()
            .map(|hardware| hardware.cpu_types.len())
            .expect("the capture fixture includes the canonical hardware projection");
        assert_eq!(
            per_core.len(),
            expected_core_count,
            "per-core history should match the canonical hardware core inventory"
        );
        assert!(
            per_core.iter().all(|samples| samples.len() >= 2),
            "the fixture supplies history for every core"
        );

        let standard_text = rendered_text_bounds(&app, standard_size, &renderer, None);
        let viewport = standard_text
            .core_grid_viewport
            .expect("the full core matrix must own a bounded scroll viewport");
        let content = standard_text
            .core_grid_content
            .expect("the core viewport must retain the complete matrix content");
        assert!(
            viewport.width > 0.0
                && viewport.height > 0.0
                && viewport.x >= 0.0
                && viewport.y >= 0.0
                && viewport.x + viewport.width <= standard_size.width
                && viewport.y + viewport.height <= standard_size.height,
            "the bounded core viewport must be nonzero and inside the standard page: {viewport:?}"
        );
        let needs_scroll = content.height > viewport.height;
        let footer_top = standard_text
            .fragments
            .iter()
            .filter(|(fragment, bounds)| {
                fragment == t("hint.footer_shortcuts") && bounds.height > 0.0
            })
            .map(|(_, bounds)| bounds.y)
            .min_by(f32::total_cmp)
            .expect("the production page footer should be present in the widget tree");
        assert!(
            viewport.y + viewport.height < footer_top,
            "the bounded core viewport must finish above the footer: viewport={viewport:?}, footer_y={footer_top}"
        );
        for index in 0..expected_core_count.min(6) {
            let label_prefix = format!("C{index:02}");
            let (label, readout) = core_cell_text_bounds(&standard_text, &label_prefix)
                .unwrap_or_else(|| {
                    panic!("the default scroll offset must contain a full {label_prefix} cell")
                });
            for (part, bounds) in [("label", label), ("readout", readout)] {
                assert!(
                    bounds.width > 0.0
                        && bounds.height > 0.0
                        && bounds.x >= viewport.x
                        && bounds.y >= viewport.y
                        && bounds.x + bounds.width <= viewport.x + viewport.width
                        && bounds.y + bounds.height <= viewport.y + viewport.height
                        && bounds.y + bounds.height < footer_top,
                    "the default zero scroll offset must show the entire first-row {part} for {label_prefix}: {bounds:?}, viewport={viewport:?}, footer_y={footer_top}"
                );
            }
            let first_row_y = core_cell_text_bounds(&standard_text, "C00")
                .expect("C00 is the canonical first CPU cell")
                .0
                .y;
            assert!(
                (label.y - first_row_y).abs() < 1.0,
                "the complete first-row cells must share one row: {label_prefix}={label:?}"
            );
        }
        for index in 0..expected_core_count {
            let label_prefix = format!("C{index:02}");
            let initial_bounds = core_label_bounds(&standard_text, &label_prefix)
                .unwrap_or_else(|| panic!("the mounted CPU page must include {label_prefix}"));
            assert!(
                initial_bounds.width > 0.0 && initial_bounds.height > 0.0,
                "every canonical core cell must have nonzero content bounds: {label_prefix}={initial_bounds:?}"
            );

            let core_bounds = if needs_scroll {
                let scroll_offset = (initial_bounds.y - viewport.y).max(0.0);
                let scrolled_text =
                    rendered_text_bounds(&app, standard_size, &renderer, Some(scroll_offset));
                core_label_bounds(&scrolled_text, &label_prefix)
                    .unwrap_or_else(|| panic!("scrolling must retain {label_prefix}"))
            } else {
                initial_bounds
            };
            assert!(
                core_bounds.width > 0.0
                    && core_bounds.height > 0.0
                    && core_bounds.x >= viewport.x
                    && core_bounds.y >= viewport.y
                    && core_bounds.x + core_bounds.width <= viewport.x + viewport.width
                    && core_bounds.y + core_bounds.height <= viewport.y + viewport.height
                    && core_bounds.x >= 0.0
                    && core_bounds.y >= 0.0
                    && core_bounds.x + core_bounds.width <= standard_size.width
                    && core_bounds.y + core_bounds.height < footer_top,
                "each core row must be fully visible inside the standard page viewport and above the footer: {label_prefix}={core_bounds:?}, viewport={viewport:?}, footer_y={footer_top}"
            );
        }
        let compact_size = Size::new(720.0, 480.0);
        let _ = app.update(Message::WindowResized(compact_size));
        let compact_budget = PerformancePageBudget::for_perf_frame(
            app.page_viewport_size(),
            app.performance.sidebar_visible,
        );
        assert_eq!(compact_budget.vertical, PerformanceVerticalRunway::Floor);
        assert_eq!(
            compact_budget.chart_inventory,
            PerformanceChartInventory::AggregateOnly,
            "compact must use the typed AggregateOnly rung"
        );
        let compact_text = rendered_text_bounds(&app, compact_size, &renderer, None);
        assert!(
            compact_text.core_grid_viewport.is_none()
                && !compact_text
                    .fragments
                    .iter()
                    .any(|(fragment, _)| fragment.starts_with("C00")),
            "compact omits the per-core grid only because its typed budget is AggregateOnly"
        );
    }

    #[test]
    fn cpu_stats_values_clear_the_embedded_four_pixel_scrollbar() {
        pin_english();
        let mut app = crate::IcedApp::demo_for_capture_with(None, None);
        let _ = app.update(Message::SelectPage(AppPage::Performance));
        let _ = app.update(Message::SelectPerfDevice(PerfDevice::Cpu));
        let size = Size::new(1180.0, 780.0);
        let _ = app.update(Message::WindowResized(size));
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        let first_value = stats
            .first()
            .and_then(StatRow::value)
            .expect("the demo CPU projection has an observed utilization value");
        let renderer = software_renderer();
        let rendered = rendered_text_bounds(&app, size, &renderer, None);
        let (viewport, _, _) = rendered
            .stats_rail
            .expect("the production CPU facts rail owns its named scroll viewport");
        let value_bounds = rendered
            .fragments
            .iter()
            .filter(|(fragment, bounds)| {
                fragment == first_value
                    && bounds.y >= viewport.y
                    && bounds.y + bounds.height <= viewport.y + viewport.height
            })
            .map(|(_, bounds)| *bounds)
            .find(|bounds| bounds.x >= viewport.x)
            .expect("the production CPU utilization value must be visible in the stats rail");
        assert!(
            value_bounds.width > 0.0
                && value_bounds.height > 0.0
                && value_bounds.x + value_bounds.width <= viewport.x + viewport.width - 8.0,
            "right-aligned stats values must clear the 4px scrollbar and 4px gap: value={value_bounds:?}, viewport={viewport:?}"
        );
    }

    #[test]
    fn cpu_facts_capture_can_scroll_to_typed_cache_rows_inside_the_stats_rail() {
        pin_english();
        let mut app = crate::IcedApp::demo();
        edit_snapshot(&mut app.shell, |snapshot| {
            let snapshot = snapshot.as_mut().expect("demo snapshot");
            snapshot.cpu.l1d_cache_kb = Some(64);
            snapshot.cpu.l1i_cache_kb = Some(64);
            snapshot.cpu.l2_cache_kb = Some(1_024);
            snapshot.cpu.l3_cache_kb = Some(16_384);
        });
        edit_hardware(&mut app.shell, |hardware| {
            hardware.as_mut().expect("demo hardware").core_breakdown = CoreBreakdown {
                p_cores: 6,
                e_cores: 8,
                lp_cores: 2,
            };
        });
        let _ = app.update(Message::SelectPage(AppPage::Performance));
        let _ = app.update(Message::SelectPerfDevice(PerfDevice::Cpu));
        let size = Size::new(1180.0, 780.0);
        let _ = app.update(Message::WindowResized(size));
        let renderer = software_renderer();
        let rendered = rendered_text_bounds_for_scroll(
            &app,
            size,
            &renderer,
            Some((PERFORMANCE_STATS_RAIL_SCROLL_ID, f32::MAX)),
        );
        let (viewport, content, translation) = rendered
            .stats_rail
            .expect("production CPU facts use the stats rail scroll owner");
        assert!(
            content.height > viewport.height
                && translation.y > 0.0
                && (translation.y - (content.height - viewport.height)).abs() <= 1.0,
            "the facts rail is genuinely scrolled to its end: viewport={viewport:?}, content={content:?}, translation={translation:?}"
        );
        let visible_strings = [
            t("cpu.performance_cores"),
            "6",
            t("cpu.efficiency_cores"),
            "8",
            t("cpu.low_power_cores"),
            "2",
            t("common.l1_data_cache"),
            "64 KiB",
            t("common.l1_instruction_cache"),
            t("common.l2_cache"),
            "1 MiB",
            t("common.l3_cache"),
            "16 MiB",
        ];
        for expected in visible_strings {
            let bounds = rendered
                .fragments
                .iter()
                .filter(|(fragment, bounds)| fragment == expected && bounds.width > 0.0)
                .map(|(_, bounds)| Rectangle {
                    x: bounds.x - translation.x,
                    y: bounds.y - translation.y,
                    ..*bounds
                })
                .find(|bounds| {
                    bounds.x >= viewport.x
                        && bounds.y >= viewport.y
                        && bounds.x + bounds.width <= viewport.x + viewport.width - 8.0
                        && bounds.y + bounds.height <= viewport.y + viewport.height
                })
                .unwrap_or_else(|| {
                    panic!("rail-end viewport must show {expected:?}: {rendered:?}")
                });
            assert!(bounds.height > 0.0);
        }
    }

    #[test]
    fn cpu_per_core_layout_keeps_rapl_and_msr_authorization_actions_in_rail_footer() {
        pin_english();
        let mut app = crate::IcedApp::demo_for_capture_with(None, None);
        let _ = app.update(Message::SelectPage(AppPage::Performance));
        let _ = app.update(Message::SelectPerfDevice(PerfDevice::Cpu));
        let rapl_attempt = app.shell.begin_rapl_power_request();
        assert!(
            app.shell
                .reject_rapl_power_request(rapl_attempt, FailureKind::RequiresEscalation)
        );
        let msr_attempt = app.shell.begin_msr_readout_request();
        assert!(
            app.shell
                .reject_msr_readout_request(msr_attempt, FailureKind::RequiresEscalation)
        );

        let standard_size = Size::new(1180.0, 780.0);
        let _ = app.update(Message::WindowResized(standard_size));
        let budget = PerformancePageBudget::for_perf_frame(
            app.page_viewport_size(),
            app.performance.sidebar_visible,
        );
        assert_eq!(budget.vertical, PerformanceVerticalRunway::Charts);
        assert_eq!(budget.chart_inventory, PerformanceChartInventory::Full);

        let renderer = software_renderer();
        let rendered = rendered_text_bounds(&app, standard_size, &renderer, None);
        let core_viewport = rendered
            .core_grid_viewport
            .expect("standard CPU composition includes the per-core viewport");
        let authorization_label_count = rendered
            .fragments
            .iter()
            .filter(|(fragment, bounds)| {
                fragment == t("settings.privileges_authorize")
                    && bounds.width > 0.0
                    && bounds.height > 0.0
            })
            .count();
        assert_eq!(
            authorization_label_count, 2,
            "the mounted CPU page must render both authorization actions"
        );

        for target in [
            FocusTarget::AuthorizeRaplPower,
            FocusTarget::AuthorizeMsrReadouts,
        ] {
            let id = Id::from(focus_id(target));
            let bounds = rendered
                .focusable_bounds
                .iter()
                .find_map(|(found, bounds)| (found == &id).then_some(*bounds))
                .unwrap_or_else(|| panic!("the production CPU page must expose {id:?}"));
            assert!(
                bounds.width > 0.0
                    && bounds.height > 0.0
                    && bounds.x >= core_viewport.x + core_viewport.width
                    && bounds.x + bounds.width <= standard_size.width,
                "authorization controls must remain nonzero in the right-side details rail: {id:?}={bounds:?}, core={core_viewport:?}"
            );

            let rail = rendered
                .scrollable_regions
                .iter()
                .find(|(viewport, content)| {
                    viewport.x >= core_viewport.x + core_viewport.width
                        && viewport.x >= 0.0
                        && viewport.y >= 0.0
                        && viewport.width > 0.0
                        && viewport.height > 0.0
                        && viewport.x + viewport.width <= standard_size.width
                        && viewport.y + viewport.height <= standard_size.height
                        && bounds.x >= content.x
                        && bounds.y >= content.y
                        && bounds.x + bounds.width <= content.x + content.width
                        && bounds.y + bounds.height <= content.y + content.height
                });
            assert!(
                rail.is_some(),
                "each authorization action must live in the bounded scrollable details rail: {id:?}={bounds:?}"
            );
        }
    }
}

#[path = "cpu_facts_projection_tests.rs"]
mod cpu_facts_projection_tests;

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

    fn thermal_status_rows(stats: &[StatRow]) -> Vec<String> {
        stats
            .iter()
            .filter(|row| row.label() == t("cpu.thermal_status"))
            .filter_map(|row| row.value().map(str::to_owned))
            .collect()
    }

    /// The real-time thermal-status / PROCHOT delivery: the Performance CPU
    /// stat column renders the shared
    /// [`taskmanager_shell::presentation::msr_thermal_status_segments`] fold
    /// beside the cumulative counters — one `Thermal status` row per node,
    /// each carrying a single `CPU {n} {state}` segment with the shared
    /// asserted/clear words and the honest dash for an unreadable register.
    /// One row per node keeps the value inside the stat column's bounded
    /// single-line slot instead of clipping the joined summary, and the whole
    /// row set is omitted while the privileged lane produced no accepted
    /// readout.
    #[test]
    fn cpu_stats_render_the_real_time_thermal_status_with_honest_absence() {
        pin_english();
        let mut app = crate::IcedApp::demo();
        // The demo bootstrap now seeds a synthetic readout through the request
        // session; close it to reach the honest-absence branch the privileged
        // lane shows before it ever runs (the fold is empty, so no row).
        app.shell.close_msr_readout_request();
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        assert!(
            thermal_status_rows(&stats).is_empty(),
            "a session that never ran must not grow a real-time row"
        );

        set_msr_thermal_status(&mut app, &[(0, Some(true)), (1, Some(false)), (2, None)]);
        let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
        let segments = msr_thermal_status_segments(app.shell.msr_readout_state());
        let expected = vec!["CPU 0 Asserted", "CPU 1 Clear", "CPU 2 —"];
        assert_eq!(
            segments, expected,
            "the accepted readout must fold per node"
        );
        let painted = thermal_status_rows(&stats);
        assert_eq!(
            painted, expected,
            "the stat rows must render one unclipped shared segment per node"
        );
        assert!(
            painted.iter().all(|value| value.chars().count() <= 20),
            "every painted segment must fit the stat column's bounded value slot: {painted:?}"
        );
        assert!(
            painted.iter().any(|value| value.contains(MISSING_VALUE)),
            "an unreadable register must keep the honest dash: {painted:?}"
        );
        assert!(
            painted
                .iter()
                .any(|value| value.contains(t("cpu.thermal_status_asserted")))
                && painted
                    .iter()
                    .any(|value| value.contains(t("cpu.thermal_status_clear"))),
            "the segments must carry the shared asserted/clear vocabulary: {painted:?}"
        );
    }
}
