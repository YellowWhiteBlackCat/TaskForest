//! The Performance-page CPU/memory overview panel: gauges + history chart +
//! summary lines, the memory-composition bar, and the Cpu-only trend strip.
//! Extracted from [`super`] so the Cpu and Memory selector tabs reuse one panel
//! without duplicating it (the history graph already carries both series).

use super::*;

mod cpu;
pub(crate) use cpu::*;
pub(crate) mod memory;
pub(crate) use memory::*;
mod projection;
mod stats;

use crate::perf_chart::PerfChart;
use crate::theme;
use iced::Length;
use iced::widget::{canvas, column, row, text};
use std::rc::Rc;
use taskmanager_core::core::metrics::CpuTemperatureSource;

use taskmanager_shell::presentation::trend::TrendSeries;
use taskmanager_shell::viewmodel::StatRow;
use taskmanager_theme::tokens;

use super::responsive::{
    DeviceNavigationPresentation, PerformanceChartInventory, PerformancePageBudget,
};
use stats::memory_stats_rows;
use taskmanager_shell::presentation::duration;

/// Dispatch the two singleton Performance resources to their named renderers.
pub(super) fn cpu_memory_detail(
    app: &crate::IcedApp,
    device: PerfDevice,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    if device == PerfDevice::Cpu {
        cpu_detail(app, budget)
    } else {
        memory_detail(app, budget)
    }
}

fn overview_gauges(app: &crate::IcedApp) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let snapshot = app.shell.projection().snapshot.as_ref();
    let cpu_value = snapshot
        .and_then(|snapshot| projection::CpuObservation::from(&snapshot.cpu).usage_pct)
        .map(|value| value.clamp(0.0, 100.0));
    let memory_value = snapshot
        .and_then(|snapshot| snapshot.memory.used_percentage_observed())
        .map(|value| value.clamp(0.0, 100.0));
    let swap_value = snapshot
        .and_then(|snapshot| snapshot.memory.swap_percentage_observed())
        .map(|value| value.clamp(0.0, 100.0));
    row![
        gauge(t("common.cpu"), cpu_value),
        gauge(t("common.memory"), memory_value),
        gauge(t("mem.swap"), swap_value),
    ]
    .spacing(12)
    .into()
}

fn cpu_detail(
    app: &crate::IcedApp,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let observed = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .map(projection::cpu_observation);
    let bogomips = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.cpu.frequency_source.is_bogomips());
    let temperature_source = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .map_or(CpuTemperatureSource::Coretemp, |snapshot| {
            snapshot.cpu.temperature_source
        });
    let headline = projection::cpu_headline_metrics(observed);
    // The chart inventory comes from the typed frame budget (both axes); no
    // local compact-flag derivation remains.
    let chart_layout = projection::CpuChartLayout::for_inventory(budget.chart_inventory);
    let extent = perf_layout::DetailExtent::for_scroll_parent(budget.device_navigation);
    let per_core_composes = chart_layout == projection::CpuChartLayout::AggregateWithPerCore
        && budget.vertical.carries_core_stack();
    let chart_height = if per_core_composes {
        // GPUI's CPU page pins the headline tier to a fixed height so the
        // per-core matrix composes INSIDE the first viewport (ICED-024-7/8);
        // a Fill height would push the core grid below the fold forever.
        Length::Fixed(cpu::HEADLINE_CHART_PRESENCE)
    } else if extent == perf_layout::DetailExtent::Fill {
        // Fixed-viewport frames hand the headline chart the column's
        // remaining height — it grows like GPUI's flex-1 headline tier.
        Length::Fill
    } else {
        // Strip frames live in the page scroll and keep the shared headline
        // floor (GPUI MAIN_GRAPH_MIN_HEIGHT parity).
        Length::Fixed(cpu::HEADLINE_CHART_FLOOR)
    };
    let mut chart_content = column![chart_time_axis(theme_snapshot)].spacing(8);
    chart_content = chart_content.push(cpu_performance_chart(app, theme_snapshot, chart_height));
    // Vertical ladder (GPUI parity): the per-chart summary rows drop
    // explicitly at the Floor rung — before the headline floor is touched.
    if budget.vertical.carries_core_stack() {
        chart_content = chart_content.push(column(cpu::cpu_graph_summary_elements(app)).spacing(2));
    }
    let chart_content = chart_content.height(extent.length());
    let mut left = vec![
        overview_gauges(app),
        // The utilization readout is carried by the CPU gauge directly above —
        // repeating it here made the strip read as two stacked rows for one
        // fact (ICED-024 S2 gauge-row optimization, ruling 2026-08-29).
        cpu_headline_readouts(
            &headline
                .iter()
                .filter(|metric| metric.kind != projection::CpuHeadlineKind::Utilization)
                .cloned()
                .collect::<Vec<_>>(),
            bogomips,
            temperature_source,
            theme_snapshot,
        ),
    ];
    if let Some(load) = app
        .shell
        .projection()
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.load_average.as_ref())
    {
        left.push(
            text(taskmanager_shell::presentation::load_average_summary(load))
                .size(f32::from(tokens::FONT_12))
                .into(),
        );
    }
    let chart_panel = perf_layout::graph_card(theme_snapshot, chart_content.into(), extent);
    match chart_layout {
        projection::CpuChartLayout::AggregateWithPerCore => {
            left.push(performance_graph_resolution_selector(
                theme_snapshot,
                app.graph_data_points(),
            ));
            left.push(chart_panel);
            // The below band renders from the Core rung up (GPUI parity).
            if budget.vertical.carries_core_stack() {
                // The family-trend strip is the Strip-frame carrier of the
                // rail sparks (its registered driver): at sidebar widths the
                // rail's own sparks carry the trends, and rendering the strip
                // here pushed the per-core matrix below the fold
                // (ICED-024-8).
                if budget.device_navigation == DeviceNavigationPresentation::Strip {
                    left.push(trend_strip_panel(app, theme_snapshot));
                }
                left.push(core_grid::per_core_grid_panel(app, theme_snapshot));
            }
        }
        projection::CpuChartLayout::AggregateOnly => left.push(chart_panel),
    }
    if let Some(card) = cpu::rapl_power_card(app, theme_snapshot) {
        left.push(card);
    }
    if let Some(card) = cpu::msr_readouts_card(app, theme_snapshot) {
        left.push(card);
    }
    let (title, subtitle, stats) = cpu_memory_header_and_stats(app, PerfDevice::Cpu);
    perf_layout::main_with_stats(
        theme_snapshot,
        title,
        subtitle,
        // CPU/Memory pages carry no device-loss fact: the family cannot
        // disappear (GPUI passes `None` too).
        None,
        left,
        stats,
        None,
        budget,
        perf_layout::DetailExtent::Fill,
    )
}

fn memory_detail(
    app: &crate::IcedApp,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let snapshot = app.shell.projection().snapshot.as_ref();
    let extent = perf_layout::DetailExtent::for_scroll_parent(budget.device_navigation);
    let chart_height = if extent == perf_layout::DetailExtent::Fill {
        Length::Fill
    } else {
        Length::Fixed(cpu::HEADLINE_CHART_FLOOR)
    };
    let mut chart_content = column![chart_time_axis(theme_snapshot)].spacing(8);
    chart_content = chart_content.push(memory_performance_chart(app, theme_snapshot, chart_height));
    if budget.vertical.carries_core_stack() {
        chart_content =
            chart_content.push(column(cpu::memory_graph_summary_elements(app)).spacing(2));
    }
    let mut left = vec![
        overview_gauges(app),
        performance_graph_resolution_selector(theme_snapshot, app.graph_data_points()),
    ];
    if let Some(snapshot) = snapshot {
        left.insert(
            1,
            perf_layout::graph_card(
                theme_snapshot,
                memory_composition_block(&snapshot.memory, theme_snapshot),
                perf_layout::DetailExtent::Content,
            ),
        );
    }
    left.push(perf_layout::graph_card(
        theme_snapshot,
        chart_content.into(),
        extent,
    ));
    // Swap-over-time headline chart (GPUI parity): gated on swap presence AND
    // the Full chart inventory so swap-less or inventory-collapsed frames keep
    // the full-height memory graph. The swap series is a percentage on the
    // memory family color at reduced alpha — distinct yet memory-adjacent.
    let has_swap = snapshot.is_some_and(|snapshot| {
        projection::MemoryObservation::from(&snapshot.memory)
            .swap_total_bytes
            .is_some_and(|total| total > 0)
    });
    if has_swap && budget.chart_inventory == PerformanceChartInventory::Full {
        let swap_samples = app.cached_swap_series();
        let mut swap_content = column![
            text(t("mem.swap"))
                .size(f32::from(tokens::FONT_13))
                .color(theme::muted_text_color(theme_snapshot))
        ]
        .spacing(8);
        let swap_chart = canvas::Canvas::new(PerfChart::new(
            Rc::clone(&swap_samples),
            Rc::from([]),
            crate::theme_binding::color(theme_snapshot.memory).scale_alpha(0.75),
            crate::theme_binding::color(theme_snapshot.palette().border),
            crate::theme_binding::color(theme_snapshot.palette().border),
            crate::perf_chart::ReadoutColors {
                bg: crate::theme_binding::color(theme_snapshot.palette().surface),
                fg: crate::theme_binding::color(theme_snapshot.palette().fg),
            },
            false,
        ))
        .width(Length::Fill)
        .height(chart_height);
        swap_content = swap_content.push(swap_chart);
        if budget.vertical.carries_core_stack() {
            let mut summary = Vec::new();
            cpu::push_graph_summary(&mut summary, t("mem.swap"), &swap_samples, |value| {
                format!("{value:.0}%")
            });
            swap_content = swap_content.push(column(summary).spacing(2));
        }
        left.push(perf_layout::graph_card(
            theme_snapshot,
            swap_content.into(),
            extent,
        ));
    }
    let (title, subtitle, stats) = cpu_memory_header_and_stats(app, PerfDevice::Memory);
    perf_layout::main_with_stats(
        theme_snapshot,
        title,
        subtitle,
        None,
        left,
        stats,
        None,
        budget,
        extent,
    )
}

/// GPUI-shaped title/subtitle and right-column readouts for CPU and Memory.
/// The left side owns the graph hierarchy; these rows keep the most important
/// scalar facts stable while the graph grows or changes overlay. Rows are
/// pre-folded shell [`StatRow`]s: an applicable-but-uncollected fact keeps its
/// row with `None` (the panel renders the shared dash dimmed); a fact that
/// does not exist on this host omits its row entirely.
pub(crate) fn cpu_memory_header_and_stats(
    app: &crate::IcedApp,
    device: PerfDevice,
) -> (String, String, Vec<StatRow>) {
    let Some(snapshot) = app.shell.projection().snapshot.as_ref() else {
        return (
            perf_device_label(device).to_string(),
            t("common.collecting_telemetry").to_string(),
            Vec::new(),
        );
    };
    if device == PerfDevice::Memory {
        let memory = &snapshot.memory;
        let observed = projection::MemoryObservation::from(memory);
        let subtitle = observed
            .total_bytes
            .map(|value| {
                format!(
                    "{} {}",
                    t("mem.total"),
                    memory_text_pref(value, app.memory_use_bytes(), app.memory_use_base2())
                )
            })
            .unwrap_or_else(|| t("common.collecting_telemetry").to_string());
        let stats = memory_stats_rows(memory, app.memory_use_bytes(), app.memory_use_base2());
        (t("common.memory").to_string(), subtitle, stats)
    } else {
        let cpu = &snapshot.cpu;
        let observed = projection::CpuObservation::from(cpu);
        let subtitle = cpu
            .brand
            .clone()
            .unwrap_or_else(|| t("common.collecting_telemetry").to_string());
        let mut stats = vec![
            StatRow::text(
                t("common.utilization"),
                observed.usage_pct.map(|value| format!("{value:.0}%")),
            ),
            cpu::cpu_speed_row(observed.frequency_mhz, cpu.frequency_source.is_bogomips()),
            cpu::cpu_temperature_row(observed.temperature_c, cpu.temperature_source),
            StatRow::text(t("common.processes"), Some(snapshot.processes.to_string())),
            StatRow::text(
                t("common.threads"),
                snapshot.threads.map(|threads| threads.to_string()),
            ),
            StatRow::text(t("common.uptime"), Some(duration(snapshot.uptime_secs))),
            StatRow::text(
                t("common.cores"),
                match (cpu.physical_cores, cpu.logical_cores) {
                    (Some(physical), Some(logical)) => Some(format!("{physical} / {logical}")),
                    _ => None,
                },
            ),
        ];
        if let Some(hardware) = app.shell.projection().hardware.as_ref() {
            if let Some(sockets) = hardware.sockets {
                stats.push(StatRow::text(
                    t("system.field.sockets"),
                    Some(sockets.to_string()),
                ));
            }
            if let Some(virt) = hardware.virt.as_deref() {
                stats.push(StatRow::text(
                    t("common.virtualization"),
                    Some(virt.to_string()),
                ));
            }
        }
        if let Some(l1d) = cpu.l1d_cache_kb {
            stats.push(StatRow::text(
                t("common.l1_data_cache"),
                Some(format_cache_kb(l1d)),
            ));
        }
        if let Some(l1i) = cpu.l1i_cache_kb {
            stats.push(StatRow::text(
                t("common.l1_instruction_cache"),
                Some(format_cache_kb(l1i)),
            ));
        }
        if let Some(l2) = cpu.l2_cache_kb {
            stats.push(StatRow::text(
                t("common.l2_cache"),
                Some(format_cache_kb(l2)),
            ));
        }
        if let Some(l3) = cpu.l3_cache_kb {
            stats.push(StatRow::text(
                t("common.l3_cache"),
                Some(format_cache_kb(l3)),
            ));
        }
        if let Some(topology) = taskmanager_shell::presentation::cpu_topology_summary(cpu) {
            stats.push(StatRow::text(t("cpu.topology"), Some(topology)));
        }
        if let Some(idle_states) = taskmanager_shell::presentation::cpu_idle_state_summary(cpu) {
            stats.push(StatRow::text(t("cpu.idle_states"), Some(idle_states)));
        }
        if let Some(interrupts) =
            taskmanager_shell::presentation::cpu_interrupt_compact_summary(cpu)
        {
            stats.push(StatRow::text(t("cpu.interrupts"), Some(interrupts)));
        }
        // Cumulative thermal-throttle trigger counters (`power.thermal-throttle-events`).
        // The whole row is omitted when no package observed a counter; the fold
        // keeps an unobserved sibling counter a labeled dash, never a zero.
        if let Some(throttle) = cpu_throttle_summary(cpu) {
            stats.push(StatRow::text(t("cpu.thermal_throttle"), Some(throttle)));
        }
        if let Some(load) = snapshot.load_average.as_ref() {
            stats.push(StatRow::text(
                t("system.load_basis"),
                Some(taskmanager_shell::presentation::load_average_basis_summary(
                    load,
                )),
            ));
        }
        if let Some(driver) = cpu.performance_policy.frequency_implementation.as_deref() {
            stats.push(StatRow::text(
                t("cpu.cpufreq_driver"),
                Some(driver.to_string()),
            ));
        }
        if let Some(governor) = cpu.performance_policy.active_policy.as_deref() {
            stats.push(StatRow::text(
                t("cpu.cpufreq_governor"),
                Some(governor.to_string()),
            ));
        }
        if let Some(preference) = cpu.performance_policy.energy_preference.as_deref() {
            stats.push(StatRow::text(
                t("cpu.power_preference"),
                Some(preference.to_string()),
            ));
        }
        if let Some(power_limits) = taskmanager_shell::presentation::cpu_power_limits_summary(cpu) {
            stats.push(StatRow::text(t("cpu.power_limits"), Some(power_limits)));
        }
        cpu::append_rapl_and_msr_stats(&app.shell, &mut stats);
        (t("common.cpu").to_string(), subtitle, stats)
    }
}

pub(crate) fn performance_graph_resolution_selector<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    current_points: usize,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let options = [60u32, 120, 300];
    let pills: Vec<Element<'a, Message, iced::Theme, iced::Renderer>> = options
        .iter()
        .map(|&pts| {
            focus::choice_pill(
                theme_snapshot,
                FocusTarget::PerformanceGraphPoints(pts),
                format!("{pts}s"),
                current_points == pts as usize,
                Message::SelectPerformanceGraphPoints(pts),
            )
        })
        .collect();
    row![
        text(t("settings.graph_data_points")).size(f32::from(tokens::FONT_11)),
        row(pills).spacing(4)
    ]
    .spacing(4)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(crate) fn format_cache_kb(kb: u64) -> String {
    if kb >= 1024 {
        let mib = kb as f64 / 1024.0;
        if (mib.fract()).abs() < 0.05 {
            format!("{mib:.0} MiB")
        } else {
            format!("{mib:.2} MiB")
        }
    } else {
        format!("{kb} KiB")
    }
}

/// The Cpu-only system-wide device trend strip: one labeled mini polyline per
/// shared series (CPU / Memory / Disk / Network / GPU), mirroring the gpui
/// sidebar sparklines. A series with fewer than two finite samples draws no
/// polyline (its caption stays) — never a fabricated flat line.
fn trend_strip_panel(
    app: &crate::IcedApp,
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    // Disk / network are raw bytes/sec: auto-scale each to its own finite peak
    // so traffic actually moves the line; the three percentage series pin max
    // at 100. Each series wears its own semantic theme color so the five mini
    // polylines read apart at a glance (matching the gpui sidebar).
    let disk = app.cached_metric_series(TrendSeries::DiskBytesPerSec);
    let disk_max = crate::trend_strip::finite_peak(&disk);
    let net = app.cached_metric_series(TrendSeries::NetworkBytesPerSec);
    let net_max = crate::trend_strip::finite_peak(&net);
    let entries = vec![
        crate::trend_strip::TrendEntry {
            caption: "CPU",
            samples: app.cached_metric_series(TrendSeries::CpuUsagePercent),
            color: crate::theme_binding::color(theme_snapshot.cpu),
            max: 100.0,
        },
        crate::trend_strip::TrendEntry {
            caption: "MEM",
            samples: app.cached_metric_series(TrendSeries::MemoryUsagePercent),
            color: crate::theme_binding::color(theme_snapshot.memory),
            max: 100.0,
        },
        crate::trend_strip::TrendEntry {
            caption: "DSK",
            samples: disk,
            color: crate::theme_binding::color(theme_snapshot.disk),
            max: disk_max,
        },
        crate::trend_strip::TrendEntry {
            caption: "NET",
            samples: net,
            color: crate::theme_binding::color(theme_snapshot.network),
            max: net_max,
        },
        crate::trend_strip::TrendEntry {
            caption: "GPU",
            samples: app.cached_metric_series(TrendSeries::GpuUsagePercent),
            color: crate::theme_binding::color(theme_snapshot.gpu),
            max: 100.0,
        },
    ];
    let strip = crate::trend_strip::TrendStrip::new(
        entries,
        crate::theme_binding::color(theme_snapshot.palette().fg_muted),
    );
    canvas::Canvas::new(strip)
        .width(Length::Fill)
        .height(Length::Fixed(crate::trend_strip::STRIP_HEIGHT))
        .into()
}

/// The CPU%/memory% time-series chart for the Performance page. The two series
/// are the shared shell `LiveGraphHistory` windows (G-02: the renderer-local
/// headline ring was retired — every frontend reads the same store, sized by
/// the persisted graph-data-points preference) and wear the accent and success
/// palette tokens (token-derived, never a literal); the panel groups the chart
/// with the gauges above and the summary below.
///
/// Too-few-samples is honest: until at least one series has two points the
/// chart shows a "collecting" placeholder instead of a fabricated flat line.
fn cpu_performance_chart(
    app: &crate::IcedApp,
    theme_snapshot: &taskmanager_theme::Theme,
    height: Length,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let cpu = app.cached_metric_series(TrendSeries::CpuUsagePercent);
    if cpu.len() < 2 {
        return text(t("common.collecting_telemetry"))
            .size(f32::from(tokens::FONT_13))
            .into();
    }
    let palette = theme_snapshot.palette();
    let cpu_color = crate::theme_binding::color(palette.accent);
    canvas::Canvas::new(PerfChart::new(
        cpu,
        Rc::from([]),
        cpu_color,
        cpu_color,
        crate::theme_binding::color(theme_snapshot.palette().border),
        crate::perf_chart::ReadoutColors {
            bg: crate::theme_binding::color(palette.surface),
            fg: crate::theme_binding::color(palette.fg),
        },
        true,
    ))
    .width(Length::Fill)
    .height(height)
    .into()
}

fn memory_performance_chart(
    app: &crate::IcedApp,
    theme_snapshot: &taskmanager_theme::Theme,
    height: Length,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let memory = app.cached_metric_series(TrendSeries::MemoryUsagePercent);
    if memory.len() < 2 {
        return text(t("common.collecting_telemetry"))
            .size(f32::from(tokens::FONT_13))
            .into();
    }
    let palette = theme_snapshot.palette();
    let memory_color = crate::theme_binding::color(palette.success);
    canvas::Canvas::new(PerfChart::new(
        memory,
        Rc::from([]),
        memory_color,
        memory_color,
        crate::theme_binding::color(theme_snapshot.palette().border),
        crate::perf_chart::ReadoutColors {
            bg: crate::theme_binding::color(palette.surface),
            fg: crate::theme_binding::color(palette.fg),
        },
        true,
    ))
    .width(Length::Fill)
    .height(height)
    .into()
}

/// Make the chronological contract visible in the chart itself. The geometry
/// and hover mapping already share the same oldest-left/newest-right helper;
/// these quiet edge labels keep a still frame from looking directionless.
fn chart_time_axis(
    theme_snapshot: &taskmanager_theme::Theme,
) -> Element<'static, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    row![
        text(t("common.earlier"))
            .size(f32::from(tokens::FONT_10))
            .color(muted),
        iced::widget::Space::new().width(Length::Fill),
        text(t("common.now"))
            .size(f32::from(tokens::FONT_10))
            .color(muted),
    ]
    .width(Length::Fill)
    .into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_overview/tests.rs"]
mod tests;
