//! CPU performance view: one dominant aggregate utilization graph above an
//! elastic per-core matrix, plus the shared pinned details surface.

mod details_panel;
mod msr_readouts;
mod package_power;
mod per_core;
mod per_core_grid;
mod stats;

pub(crate) use msr_readouts::MsrReadoutsInputs;
pub(crate) use package_power::PackagePowerInputs;

use msr_readouts::MsrReadoutsModel;
use package_power::PackagePowerModel;

/// The escalation-backed readout subsections of the CPU details panel,
/// projected once per render and painted in a fixed order (package power,
/// then MSR readouts). One argument so the pinned renderer stays inside the
/// shared argument budget.
pub(crate) struct EscalationReadouts {
    pub package_power: PackagePowerModel,
    pub msr_readouts: MsrReadoutsModel,
}

// Single source for the CPUID identity rows, the P/E/LP core-class row
// projection and the socket-count fold: the CPU page's details panel and the
// System page's CPU section all consume them (ADR-020). The pure spec-row
// builder `cpu_spec_rows` stays CPU-page-internal until a second surface
// genuinely shares its formats.
pub(super) use details_panel::{cpu_identity_rows, heterogeneous_core_rows, sockets_row};

use gpui::{Context, Div, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_telemetry_store::TelemetryStore;

use crate::gpui_app::root::RootView;

use crate::gpui_app::formatting::{self, GraphUnit};
use crate::gpui_app::graph::GraphHover;
use crate::gpui_app::perf_views::{ChartSpec, HeadlineSurface, PerfPageProps, perf_page};
use crate::gpui_app::root::responsive::{PerformanceChartInventory, PerformancePageBudget};
pub(crate) use per_core::CpuHistoryCache;
pub(crate) use per_core::per_core_cell_label;
use stats::CpuLiveStats;
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::metrics::{
    CpuFrequencySource, CpuMetrics, CpuTemperatureSource, SystemSnapshot,
};
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

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
    pub stats_scroll: gpui::ScrollHandle,
    pub snap: &'a SystemSnapshot,
    pub telemetry: &'a TelemetryStore,
    pub hardware: &'a HardwareInfo,
    pub hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub graph_settings: crate::gpui_app::graph::GraphSettings,
    pub layout: PerformancePageBudget,
    /// Presentation unit preferences for the details panel spec list.
    pub units: UnitPreferences,
    /// Shared package-power request session + lane capability for the
    /// details panel's package-power subsection.
    pub package_power: PackagePowerInputs<'a>,
    /// Shared MSR-readout request session + lane capability for the details
    /// panel's MSR subsection.
    pub msr_readouts: MsrReadoutsInputs<'a>,
}

pub(crate) fn render_cpu(
    props: CpuViewProps<'_>,
    core_history: &mut CpuHistoryCache,
    cx: &mut Context<RootView>,
) -> Div {
    let CpuViewProps {
        theme,
        stats_scroll,
        snap,
        telemetry,
        hardware,
        hover_slot,
        graph_settings,
        layout,
        units,
        package_power,
        msr_readouts,
    } = props;
    let package_model = package_power::package_power_model(&package_power);
    let msr_model = msr_readouts::msr_readouts_model(&msr_readouts);
    let escalation = EscalationReadouts {
        package_power: package_model,
        msr_readouts: msr_model,
    };
    let cpu = &snap.cpu;
    let stats = CpuLiveStats::from_snapshot(snap);
    let chart_layout = CpuChartLayout::for_inventory(layout.chart_inventory);
    let aggregate_series = core_history.aggregate(telemetry);
    let core_series = core_history.refresh(telemetry);
    // The per-core matrix is the Full-inventory companion; an aggregate-only
    // budget keeps one readable headline chart.
    //
    // Vertical page policy: the matrix composes only when its summed row
    // floors fit beneath the headline's floor — a band that cannot meet its
    // minimums hides WHOLE and hands the viewport to the aggregate graph
    // (never squeezed half-visible rows). `content_height == 0.0` is the
    // legacy no-frame budget: fall back to the coarse inventory gate only.
    let matrix_min_height = per_core_grid::min_height(&stats, hardware);
    let matrix_fits = layout.content_height == 0.0
        || layout.content_height >= CPU_VERTICAL_CHROME + HEADLINE_FLOOR + matrix_min_height;
    let below = match chart_layout {
        CpuChartLayout::AggregateOnly => None,
        CpuChartLayout::AggregateWithPerCore if matrix_fits => Some(
            per_core_grid::render(theme, &stats, hardware, &core_series, graph_settings)
                .into_any_element(),
        ),
        CpuChartLayout::AggregateWithPerCore => None,
    };
    // Headline-stays-small: while the matrix carries the page, the aggregate
    // chart is capped — it is the clear-at-a-glance surface. When the matrix
    // hides, the cap lifts and the headline fills the viewport.
    let mut headline = ChartSpec::headline(
        "cpu-headline-graph",
        "cpu-headline-graph",
        aggregate_series.usage,
        theme.cpu,
        GraphUnit::Percent,
    );
    if below.is_some() {
        headline = headline.with_max_height(px(HEADLINE_CAP_WITH_MATRIX));
    }
    perf_page(PerfPageProps {
        theme,
        stats_scroll,
        title: i18n::t("common.cpu").to_owned(),
        subtitle: cpu_brand(cpu),
        vital_line: None,
        header_extra: Some(readout_band(theme, &stats).into_any_element()),
        headline: HeadlineSurface::Charts(vec![headline]),
        below,
        stats: details_panel::render_pinned(
            theme,
            snap,
            hardware,
            &stats.details,
            units,
            &escalation,
            cx,
        ),
        stats_footer: None,
        hover_slot,
        graph_settings,
        budget: layout,
    })
}

/// Fixed vertical chrome above the chart area: the title row, the readout
/// band, the headline's summary row, and the main-column gaps. Generous by
/// a small margin — it only gates a whole-band hide, never a squeeze.
const CPU_VERTICAL_CHROME: f32 = 150.0;
/// The headline card's floor while the matrix is visible. Lower than the
/// 180px solo tier floor: the aggregate graph is small by default and the
/// matrix carries the page's information density.
const HEADLINE_FLOOR: f32 = 140.0;
/// The headline card's ceiling while the per-core matrix is visible.
const HEADLINE_CAP_WITH_MATRIX: f32 = 240.0;

fn cpu_brand(cpu: &CpuMetrics) -> String {
    cpu.brand
        .as_deref()
        .map(str::trim)
        .filter(|brand| !brand.is_empty())
        .map_or_else(formatting::missing_value, str::to_string)
}

/// The header band under the title row: the always-visible big-number
/// readouts (utilization, frequency, temperature, power). No separate
/// caption row — a full-width row for a caption starved the chart area, and
/// the readouts' own labels already say what they measure.
fn readout_band(theme: &Theme, stats: &CpuLiveStats) -> Div {
    div().flex().flex_col().child(readouts(theme, stats))
}

fn readouts(theme: &Theme, stats: &CpuLiveStats) -> Div {
    let mut strip = div()
        .debug_selector(|| "tm-cpu-readouts".to_string())
        .flex()
        .flex_wrap()
        // Center, not baseline: gpui's baseline alignment through nested
        // divs is unreliable with mixed font sizes, and the drifted label
        // row was visibly uneven (labels sit one line-height low).
        .items_center()
        .gap(tokens::SPACE_16)
        .child(readout(
            theme,
            i18n::t("common.utilization"),
            stats.utilization_readout.clone(),
            true,
        ));
    if let Some(frequency) = &stats.frequency_readout {
        strip = strip.child(readout(
            theme,
            i18n::t("cpu.frequency"),
            frequency.clone(),
            false,
        ));
    }
    if let Some(temperature) = &stats.temperature_readout {
        strip = strip.child(readout(
            theme,
            i18n::t("common.temperature"),
            temperature.clone(),
            false,
        ));
    }
    if let Some(power) = &stats.power_readout {
        strip = strip.child(readout(
            theme,
            i18n::t("common.power"),
            power.clone(),
            false,
        ));
    }
    strip
}

fn readout(theme: &Theme, label: &str, value: String, primary: bool) -> Div {
    div()
        .flex()
        .items_center()
        .gap(tokens::SPACE_6)
        .child(
            div()
                .text_size(tokens::FONT_13)
                .font_weight(tokens::FONT_WEIGHT_BOLD.into())
                .text_color(if primary { theme.fg } else { theme.fg_dim })
                .child(label.to_owned()),
        )
        .child(
            div()
                .text_size(if primary {
                    tokens::FONT_20
                } else {
                    tokens::FONT_18
                })
                .font_weight(tokens::FONT_WEIGHT_EXTRA_BOLD.into())
                .text_color(theme.fg)
                .child(value),
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
