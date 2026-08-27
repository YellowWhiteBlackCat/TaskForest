//! CPU performance view: one dominant aggregate utilization graph above an
//! elastic per-core matrix, plus the shared pinned details surface.

mod aggregate;
mod details_panel;
mod per_core;
mod per_core_grid;
mod stats;

// Single source for the P/E/LP core-class row projection and the socket-count
// fold: the CPU page's details panel and the System page's CPU section both
// consume them (ADR-020). The pure spec-row builder `cpu_spec_rows` stays
// CPU-page-internal until a second surface genuinely shares its formats.
pub(super) use details_panel::{heterogeneous_core_rows, sockets_row};

use gpui::{Div, InteractiveElement, ParentElement, ScrollHandle, Styled, div, px};
use taskmanager_telemetry_store::TelemetryStore;

use crate::core::hardware::HardwareInfo;
use crate::core::metrics::{CpuFrequencySource, CpuMetrics, CpuTemperatureSource, SystemSnapshot};
use crate::gpui_app::elements;
use crate::gpui_app::formatting;
use crate::gpui_app::graph::{GraphHover, GraphSettings, graph_hover};
use crate::gpui_app::root::responsive::{
    PerformanceChartInventory, PerformanceDetailsPresentation, PerformancePageBudget,
};
use crate::gpui_app::theme::Theme;
use crate::gpui_app::theme::tokens;
use crate::i18n;
pub(crate) use per_core::CpuHistoryCache;
pub(crate) use per_core::per_core_cell_label;
use stats::CpuLiveStats;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CpuChartLayout {
    AggregateWithPerCore,
    AggregateOnly,
}

impl CpuChartLayout {
    fn for_inventory(inventory: PerformanceChartInventory) -> Self {
        match inventory {
            PerformanceChartInventory::AggregateOnly => Self::AggregateOnly,
            PerformanceChartInventory::Full => Self::AggregateWithPerCore,
        }
    }
}

pub(super) fn cpu_frequency_readout(frequency_mhz: Option<u64>) -> String {
    crate::gpui_app::formatting::optional_ghz(frequency_mhz)
}

fn cpu_frequency_readout_for_source(
    frequency_value: Option<u64>,
    source: CpuFrequencySource,
) -> String {
    match (frequency_value, source) {
        (Some(value), CpuFrequencySource::BogoMips) => format!("{value}.00 BogoMIPS"),
        (frequency, CpuFrequencySource::Native) => cpu_frequency_readout(frequency),
        (None, CpuFrequencySource::BogoMips) => formatting::missing_value(),
    }
}

pub(super) fn cpu_temperature_readout(temperature_c: Option<f32>) -> String {
    temperature_c.map_or_else(formatting::missing_value, |temperature| {
        format!("{:.0} °C", temperature.round())
    })
}

/// i18n note key for a temperature source tier that needs a visible
/// qualifier; native chip drivers stay unqualified.
fn cpu_temperature_source_note_key(source: CpuTemperatureSource) -> Option<&'static str> {
    match source {
        CpuTemperatureSource::PackageHwmon => Some("cpu.temperature_source.package_hwmon"),
        CpuTemperatureSource::ThermalZone => Some("cpu.temperature_source.thermal_zone"),
        _ => None,
    }
}

/// One temperature readout formatted for its source (the BogoMIPS
/// `cpu_frequency_readout_for_source` counterpart): a labeled-fallback tier
/// appends the source qualifier so a CPU-package-labeled channel on another
/// hwmon chip — or an ACPI thermal zone — never masquerades as a dedicated
/// CPU sensor chip. Native chips and missing values are unchanged.
fn cpu_temperature_readout_for_source(
    temperature_c: Option<f32>,
    source: CpuTemperatureSource,
) -> String {
    let readout = cpu_temperature_readout(temperature_c);
    match cpu_temperature_source_note_key(source) {
        Some(key) if temperature_c.is_some() => format!("{readout} · {}", i18n::t(key)),
        _ => readout,
    }
}

/// All straight-through CPU-page render inputs (design-debt #1 props
/// consolidation). `core_history` stays a separate `&mut` parameter: it is
/// per-window cache state mutated during render, not a read-only input.
pub(crate) struct CpuViewProps<'a> {
    pub theme: &'a Theme,
    pub stats_scroll: ScrollHandle,
    pub snap: &'a SystemSnapshot,
    pub telemetry: &'a TelemetryStore,
    pub hardware: &'a HardwareInfo,
    pub hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub graph_settings: GraphSettings,
    pub layout: PerformancePageBudget,
}

pub(crate) fn render_cpu(props: CpuViewProps<'_>, core_history: &mut CpuHistoryCache) -> Div {
    let CpuViewProps {
        theme,
        stats_scroll,
        snap,
        telemetry,
        hardware,
        hover_slot,
        graph_settings,
        layout,
    } = props;
    let cpu = &snap.cpu;
    let stats = CpuLiveStats::from_snapshot(snap);
    let chart_layout = CpuChartLayout::for_inventory(layout.chart_inventory);
    let aggregate_series = core_history.aggregate(telemetry);
    let core_series = core_history.refresh(telemetry);
    let mut left = div()
        .debug_selector(|| "tm-cpu-chart-surface".to_string())
        .flex()
        .flex_col()
        .gap(tokens::SPACE_10)
        .flex_1()
        .min_w(px(0.0))
        .min_h(px(0.0))
        .w_full()
        // Performance's outer trailing inset is intentionally zero because
        // the details rail reaches the page edge. The chart column owns this
        // internal inset so graph ink never touches the divider.
        .pr(px(layout.main_trailing_inset))
        .child(header(theme, cpu))
        .child(aggregate::render(aggregate::AggregateGraphsProps {
            theme,
            stats: &stats,
            series: aggregate_series,
            hover_slot,
            graph_settings,
            layout: chart_layout,
        }));
    left = match chart_layout {
        CpuChartLayout::AggregateOnly => left,
        CpuChartLayout::AggregateWithPerCore => left.child(per_core_grid::render(
            theme,
            &stats,
            hardware,
            &core_series,
            graph_settings,
        )),
    };
    if let Some((position, text)) = graph_hover(hover_slot) {
        left = left.child(elements::tooltip_overlay(theme, &text, position));
    }
    match layout.details {
        PerformanceDetailsPresentation::Hidden => left,
        PerformanceDetailsPresentation::Pinned => crate::gpui_app::perf_views::performance_split(
            theme,
            left,
            details_panel::render_pinned(theme, snap, hardware, &stats.details),
            stats_scroll,
        ),
    }
}

fn header(theme: &Theme, cpu: &CpuMetrics) -> Div {
    let brand = cpu
        .brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(formatting::missing_value, str::to_string);
    let subtitle_key = "cpu.utilization_over_60s";
    div()
        .flex()
        .flex_col()
        .gap(tokens::SPACE_2)
        .child(crate::gpui_app::perf_views::performance_title_row(
            theme,
            i18n::t("common.cpu").to_owned(),
            brand,
        ))
        .child(
            div()
                .text_size(tokens::FONT_12)
                .text_color(theme.fg_dim)
                .child(i18n::t(subtitle_key)),
        )
}

pub fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    let s = secs % 60;
    if d > 0 {
        format!("{}:{:02}:{:02}:{:02}", d, h, m, s)
    } else {
        format!("{:02}:{:02}:{:02}", h, m, s)
    }
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_cpu_view_topology_tests.rs"]
mod topology_tests;
