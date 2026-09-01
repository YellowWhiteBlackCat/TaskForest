//! Full per-device Performance views (Memory / Disk / Network / GPU), replacing the
//! minimal placeholder. Each is MC-styled: title + subtitle + main graph (+ composition
//! bar for memory) on the left, stat panel on the right.

use gpui::{
    AnyElement, Context, Div, ElementId, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div,
};
use taskmanager_telemetry_store::TelemetryStore;

use crate::gpui_app::elements;
use crate::gpui_app::formatting::{
    GraphUnit, PerformanceSettings, format_drive_graph_megabytes, format_network_graph_megabytes,
};
use crate::gpui_app::graph::GraphHover;
use crate::gpui_app::history_samples::{
    f32_history_samples, network_rate_samples, network_rx_rate_samples, network_tx_rate_samples,
    storage_activity_samples, storage_rate_samples, storage_read_rate_samples,
    storage_temperature_samples, storage_write_rate_samples,
};
use crate::gpui_app::root::RootView;
use crate::gpui_app::root::responsive::{
    COMPACT_CONTENT_WIDTH, PerformanceChartInventory, PerformancePageBudget,
};
use std::cell::RefCell;
use std::rc::Rc;
use taskmanager_application::i18n;
use taskmanager_core::core::DirectoryUsageSnapshot;
use taskmanager_core::core::metrics::{DiskMetrics, NetworkAdapterType, SystemSnapshot};
use taskmanager_core::core::units::{QuantityFamily, UnitPreferences};
use taskmanager_theme::Theme;

mod directory_usage;
mod disk_stats;
mod dynamic;
mod dynamic_stats;
pub(crate) mod gpu_engines_panel;
mod gpu_page;
mod gpu_stats;
pub(crate) mod history_replay;
mod layout;
pub use layout::PERF_MAIN_VIEWPORT_SELECTOR;
mod memory_composition;
mod memory_details;
mod memory_stats;
mod network_stats;
mod partition_stats;
mod smart_dialog;
mod smart_status;
use disk_stats::disk_stats;
pub(crate) use dynamic::{BatteryViewProps, FanViewProps, render_battery, render_fan};
use taskmanager_theme::tokens;
// The ONE page composition root and its chart specification are the crate's
// public surface for Performance pages: cpu_view composes through the same
// root as every device page, so no sibling module can hand-roll a parallel
// page shell.
pub(crate) use layout::{
    ChartSpec, DualLanes, HEADLINE_COMPANION_FLOOR, HeadlineSurface, PerfPageProps, perf_page,
};
use layout::{render_chart, stats_panel};
use memory_composition::{
    MEMORY_DETAIL_COMPACT_MIN_HEIGHT, MEMORY_DETAIL_FULL_MIN_HEIGHT, MemoryDetailMode,
    composition_block,
};
use memory_stats::memory_page_stats;
use network_stats::{network_link_speed_graph_max, network_stats, network_title};
use partition_stats::MAX_VISIBLE_MOUNTED_PARTITIONS;
use partition_stats::partition_panel;
pub use smart_dialog::render_smart_dialog;
use smart_status::status_footer;
use taskmanager_shell::presentation::{
    device_status_i18n_key, effective_smart_status, has_smart_fields, smart_section_visible,
};

pub(crate) use gpu_page::{GpuChartLayout, GpuRenderState, gpu_percentage_readout, render_gpu};

/// Memory page's fixed chrome above the chart area: the title row and the
/// main-column gaps. Generous by intent — it only gates a mode change.
const MEMORY_TITLE_CHROME: f32 = 60.0;
/// One headline chart's floor: the 180px card plus its summary row.
const MEMORY_CHART_FLOOR_ONE: f32 = 200.0;
/// Both headline charts' floors plus the inter-chart gap.
const MEMORY_CHART_FLOOR_BOTH: f32 = 2.0 * MEMORY_CHART_FLOOR_ONE + 10.0;
/// Fit-check safety margin: the compact detail's real height drifts a few px
/// with text metrics. The margin keeps that drift from clipping the swap
/// chart's tail against the fixed viewport.
const MEMORY_FIT_SAFETY: f32 = 24.0;

/// Disk lower-band fit bounds. The main Performance viewport is fixed and
/// non-scrolling, so the partition panel owns the first lower-band slot;
/// activity and directory analysis are admitted only when their complete
/// footprints also fit. These constants deliberately overestimate text
/// height to keep the final card away from the viewport edge.
const DISK_TOP_CHROME_FLOOR: f32 = 86.0;
const DISK_HEADLINE_SECTION_FLOOR: f32 = 186.0;
const DISK_PARTITION_PANEL_BASE: f32 = 68.0;
const DISK_PARTITION_ROW_FLOOR: f32 = 58.0;
const DISK_PARTITION_SUMMARY_FLOOR: f32 = 24.0;
const DISK_ACTIVITY_SECTION_FLOOR: f32 = 170.0;
const DISK_USAGE_PANEL_FLOOR: f32 = 280.0;
const DISK_LOWER_GAP: f32 = 8.0;
const DISK_BOTTOM_SAFETY: f32 = 12.0;

#[cfg(test)]
#[path = "../../tests/gui/gpui_app/perf_views/tests.rs"]
mod tests;

// ── graph value-badge formatters ─────────────────────────────────────────────
// Plain `fn(f32) -> String` pointers (not closures) so they slot into
// `GraphOpts::badge_fmt` while keeping `GraphOpts` `Copy`. One per series unit;
// the top-right pill thus reads in the graph's native unit (memory/GPU = %,
// storage activity/GPU/memory = %, network = MB/s).
pub(super) fn badge_pct(v: f32) -> String {
    format!("{v:.0}%")
}
fn badge_network_bytes_decimal(v: f32) -> String {
    format_network_graph_megabytes(
        UnitPreferences {
            network_use_bytes: true,
            network_use_base2: false,
            ..UnitPreferences::default()
        },
        v,
    )
}

fn badge_network_bytes_binary(v: f32) -> String {
    format_network_graph_megabytes(
        UnitPreferences {
            network_use_bytes: true,
            network_use_base2: true,
            ..UnitPreferences::default()
        },
        v,
    )
}

fn badge_network_bits_decimal(v: f32) -> String {
    format_network_graph_megabytes(
        UnitPreferences {
            network_use_bytes: false,
            network_use_base2: false,
            ..UnitPreferences::default()
        },
        v,
    )
}

fn badge_network_bits_binary(v: f32) -> String {
    format_network_graph_megabytes(
        UnitPreferences {
            network_use_bytes: false,
            network_use_base2: true,
            ..UnitPreferences::default()
        },
        v,
    )
}

pub(super) fn network_badge_format(units: UnitPreferences) -> fn(f32) -> String {
    match (units.network_use_bytes, units.network_use_base2) {
        (true, false) => badge_network_bytes_decimal,
        (true, true) => badge_network_bytes_binary,
        (false, false) => badge_network_bits_decimal,
        (false, true) => badge_network_bits_binary,
    }
}

fn badge_drive_bytes_decimal(v: f32) -> String {
    format_drive_graph_megabytes(
        UnitPreferences {
            drive_use_bytes: true,
            drive_use_base2: false,
            ..UnitPreferences::default()
        },
        v,
    )
}

fn badge_drive_bytes_binary(v: f32) -> String {
    format_drive_graph_megabytes(
        UnitPreferences {
            drive_use_bytes: true,
            drive_use_base2: true,
            ..UnitPreferences::default()
        },
        v,
    )
}

fn badge_drive_bits_decimal(v: f32) -> String {
    format_drive_graph_megabytes(
        UnitPreferences {
            drive_use_bytes: false,
            drive_use_base2: false,
            ..UnitPreferences::default()
        },
        v,
    )
}

fn badge_drive_bits_binary(v: f32) -> String {
    format_drive_graph_megabytes(
        UnitPreferences {
            drive_use_bytes: false,
            drive_use_base2: true,
            ..UnitPreferences::default()
        },
        v,
    )
}

/// The value-badge formatter for the Drive-rate graph family, mirroring
/// [`network_badge_format`]'s unit-pair resolution on the drive preference
/// fields.
pub(super) fn drive_badge_format(units: UnitPreferences) -> fn(f32) -> String {
    match (units.drive_use_bytes, units.drive_use_base2) {
        (true, false) => badge_drive_bytes_decimal,
        (true, true) => badge_drive_bytes_binary,
        (false, false) => badge_drive_bits_decimal,
        (false, true) => badge_drive_bits_binary,
    }
}
fn badge_rpm(v: f32) -> String {
    taskmanager_shell::presentation::fan_rpm(v)
}
fn badge_watts(v: f32) -> String {
    taskmanager_shell::presentation::power_w(v)
}
fn badge_temperature(v: f32) -> String {
    taskmanager_shell::presentation::temperature_c(v)
}
/// Generation-keyed cache of the Memory page header-chart sample projections.
///
/// Memory + swap histories change only when the platform batch accepts a
/// Memory-domain system outcome, so every other render (hover, keyboard,
/// resize, overlay switch) reuses the previous projection instead of
/// re-cloning the correlated histories out of the telemetry store and
/// re-projecting them to `f32` samples. `bump()` is called from the
/// batch-apply path; the projection is rebuilt lazily on the next render
/// that sees an advanced generation — the same keying as
/// `cpu_view::CpuHistoryCache`.
pub(crate) struct MemoryHistoryCache {
    /// Generation the cached projection was built at; `None` until the first
    /// render, so an empty cache always builds.
    built_at_generation: Option<u64>,
    /// Monotonic generation. Advanced once per accepted Memory-domain outcome.
    generation: u64,
    /// Full (pre-`limit_samples`) memory-usage projection (oldest..newest,
    /// `NaN` = gap). `Rc` so a cache hit hands the graph + summary row a
    /// shared slice without cloning the sample storage.
    memory_samples: Rc<[f32]>,
    /// Full (pre-`limit_samples`) swap-usage projection.
    swap_samples: Rc<[f32]>,
}

impl MemoryHistoryCache {
    pub(crate) fn new() -> Self {
        Self {
            built_at_generation: None,
            generation: 0,
            memory_samples: Vec::new().into(),
            swap_samples: Vec::new().into(),
        }
    }

    /// The store accepted a Memory-domain outcome; the next render must
    /// rebuild the projection.
    pub(crate) fn bump(&mut self) {
        self.generation += 1;
    }

    /// Render entry: rebuild the memory + swap projections only when the
    /// generation advanced since the last build, then return them as borrows.
    /// Each borrow derefs to `&[f32]` for the downstream `limit_samples`
    /// call, so a cache hit performs zero sample-storage allocation.
    pub(crate) fn refresh(&mut self, telemetry: &TelemetryStore) -> (&Rc<[f32]>, &Rc<[f32]>) {
        if self.built_at_generation != Some(self.generation) {
            self.built_at_generation = Some(self.generation);
            self.memory_samples =
                Rc::from(f32_history_samples(telemetry.system_history.memory_usage()));
            self.swap_samples =
                Rc::from(f32_history_samples(telemetry.system_history.swap_usage()));
        }
        (&self.memory_samples, &self.swap_samples)
    }
}

/// Stateless renderer inputs for the Memory detail page plus its generation
/// keyed history projection cache.
pub(crate) struct MemoryViewProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) snap: &'a SystemSnapshot,
    pub(crate) telemetry: &'a TelemetryStore,
    pub(crate) performance: PerformanceSettings,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub(crate) memory_history: &'a mut MemoryHistoryCache,
    pub(crate) budget: PerformancePageBudget,
}

pub(crate) fn render_memory(props: MemoryViewProps<'_>) -> Div {
    let MemoryViewProps {
        theme,
        snap,
        telemetry,
        performance,
        hover_slot,
        memory_history,
        budget,
    } = props;
    let units = performance.units;
    let m = &snap.memory;
    let memory_stats = memory_page_stats(m, units);
    let stats = memory_stats.rows;

    // One fixed main viewport (the shared device-page contract): the memory
    // headline and — when the host has swap and the chart-inventory budget
    // permits — the swap headline are `flex_1` siblings that split the
    // column's remaining height evenly. The full memory + swap projections
    // are generation-keyed (see `MemoryHistoryCache`): a cache hit on a
    // UI-only frame reuses the last projection via `Rc` deref, and the
    // chart's tail-limit keeps the identity when the window already fits.
    let (mem_history, swap_history) = memory_history.refresh(telemetry);
    // Vertical page policy (memory's own ladder, 求同存异): the two headline
    // charts keep their floors for as long as possible; the multi-line
    // composition detail degrades TEXT-first — Full, then Compact (caption +
    // composition bar, prose omitted), then gone (the Floor rung drops the
    // header band wholesale). A swap chart that cannot meet its floor hides
    // instead of squeezing. The Full prose form additionally requires a wide
    // column: a narrow main column wraps the prose and the card grows past
    // any height budget. `content_height == 0.0` is the legacy no-frame
    // budget: unlimited, today's behavior.
    let legacy_budget = budget.content_height <= 0.0;
    let available = if legacy_budget {
        f32::INFINITY
    } else {
        (budget.content_height - MEMORY_TITLE_CHROME).max(0.0)
    };
    let width_admits_full_prose = budget.workspace_width >= COMPACT_CONTENT_WIDTH;
    let keep_swap = memory_stats.has_swap
        && budget.chart_inventory == PerformanceChartInventory::Full
        && available
            >= MEMORY_DETAIL_COMPACT_MIN_HEIGHT + MEMORY_CHART_FLOOR_BOTH + MEMORY_FIT_SAFETY;
    let charts_floor = if keep_swap {
        MEMORY_CHART_FLOOR_BOTH
    } else {
        MEMORY_CHART_FLOOR_ONE
    };
    let detail_mode =
        if width_admits_full_prose && available >= MEMORY_DETAIL_FULL_MIN_HEIGHT + charts_floor {
            MemoryDetailMode::Full
        } else {
            MemoryDetailMode::Compact
        };
    let detail_floor = match detail_mode {
        MemoryDetailMode::Full => MEMORY_DETAIL_FULL_MIN_HEIGHT,
        MemoryDetailMode::Compact => MEMORY_DETAIL_COMPACT_MIN_HEIGHT,
    };
    // The lower card is admitted only when its complete compact/full footprint
    // fits beside the chart floors. The fixed Performance viewport has no
    // scroll escape hatch, so a lower card that cannot fit is omitted whole;
    // it must never leave a half-painted bar at the bottom edge.
    let show_detail = legacy_budget
        || (budget.vertical.carries_below()
            && available >= charts_floor + detail_floor + MEMORY_FIT_SAFETY);
    let mut charts = vec![ChartSpec::headline(
        "mem-graph",
        "mem-graph",
        Rc::clone(mem_history),
        theme.memory,
        GraphUnit::Percent,
    )];
    // Swap-over-time graph — gated on swap presence (and the Full chart
    // inventory) so swap-less or compact hosts keep the full-height memory
    // graph. The correlated swap series is a percentage (0..=100) and reuses
    // theme.memory at a lower alpha so it reads as distinct yet
    // memory-adjacent.
    if keep_swap {
        charts.push(
            ChartSpec::headline(
                "swap-graph",
                "swap-graph",
                Rc::clone(swap_history),
                theme.memory.with_alpha(0.75),
                GraphUnit::Percent,
            )
            .with_title(i18n::t("mem.swap")),
        );
    }
    perf_page(PerfPageProps {
        theme,
        title: i18n::t("common.memory").into(),
        subtitle: format!("{} {}", i18n::t("mem.total"), memory_stats.total_readout),
        vital_line: None,
        header_extra: None,
        headline: HeadlineSurface::Charts(charts),
        // The composition module lives BELOW the two headline charts (the
        // memory page's own ordering, matching disk: charts carry the page,
        // the detail module follows and yields first).
        below: show_detail
            .then(|| composition_block(theme, m, units, detail_mode).into_any_element()),
        stats: stats_panel(theme, stats, budget.details, budget.content_height),
        stats_footer: None,
        hover_slot,
        graph_settings: performance.graph,
        budget,
    })
}

/// Stateless renderer props for the Disk page. The props boundary prevents
/// the disk renderer from growing another independent argument for every
/// panel family (mirrors `CpuViewProps`).
pub(crate) struct DiskViewProps<'a> {
    pub theme: &'a Theme,
    pub snap: &'a SystemSnapshot,
    pub telemetry: &'a TelemetryStore,
    pub index: usize,
    pub performance: PerformanceSettings,
    pub directory_usage: Option<&'a DirectoryUsageSnapshot>,
    pub hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub budget: PerformancePageBudget,
}

fn disk_partition_panel_floor(disk: &DiskMetrics) -> f32 {
    let mounted = disk
        .partitions
        .iter()
        .filter(|partition| !partition.mount_point.trim().is_empty())
        .count();
    let unmounted = disk
        .partitions
        .iter()
        .any(|partition| partition.mount_point.trim().is_empty());
    let mut height = DISK_PARTITION_PANEL_BASE;
    for _ in 0..mounted.min(MAX_VISIBLE_MOUNTED_PARTITIONS) {
        height += DISK_PARTITION_ROW_FLOOR + DISK_LOWER_GAP;
    }
    if mounted > MAX_VISIBLE_MOUNTED_PARTITIONS {
        height += DISK_PARTITION_SUMMARY_FLOOR + DISK_LOWER_GAP;
    }
    if unmounted {
        height += DISK_PARTITION_SUMMARY_FLOOR + DISK_LOWER_GAP;
    }
    height
}

fn disk_lower_band_capacity(content_height: f32) -> Option<f32> {
    if content_height <= 0.0 {
        return None;
    }
    Some(
        (content_height - DISK_TOP_CHROME_FLOOR - DISK_HEADLINE_SECTION_FLOOR - DISK_BOTTOM_SAFETY)
            .max(0.0),
    )
}

fn lower_band_fits(capacity: Option<f32>, required: f32) -> bool {
    capacity.is_none_or(|available| available >= required)
}

pub(crate) fn render_disk(props: DiskViewProps<'_>, cx: &mut Context<RootView>) -> Div {
    let DiskViewProps {
        theme,
        snap,
        telemetry,
        index: i,
        performance,
        directory_usage,
        hover_slot,
        budget,
    } = props;
    let units = performance.units;
    let Some(d) = snap.disks.get(i) else {
        return div();
    };
    // Split-direction throughput windows (read = family token, write = the
    // same token lifted toward white) plus the summed lane that keeps the
    // aggregate summary and first-frame state honest. Each direction keeps
    // its own gap evidence; the shared max is the greater finite peak of the
    // two windows so the directions stay directly comparable.
    let read_samples =
        storage_read_rate_samples(&telemetry.system_history, &d.device_id, d.device_generation);
    let write_samples =
        storage_write_rate_samples(&telemetry.system_history, &d.device_id, d.device_generation);
    let samples =
        storage_rate_samples(&telemetry.system_history, &d.device_id, d.device_generation);
    let observed_max = finite_series_peak(&read_samples).max(finite_series_peak(&write_samples));
    let temperature_samples =
        storage_temperature_samples(&telemetry.system_history, &d.device_id, d.device_generation);
    let activity_samples =
        storage_activity_samples(&telemetry.system_history, &d.device_id, d.device_generation);
    // Active-time percentage window beneath the throughput pair (the battery
    // power / fan temperature secondary-chart precedent): this disk's own
    // generation-scoped activity ring on the shared 0..100 percent scale. The
    // card appears only when the ring holds samples AND the chart-inventory
    // budget keeps secondary charts — a cold window or a platform whose
    // active-time source is honestly unavailable keeps the absence instead
    // of a fabricated flat 0%.
    let partition_floor = disk_partition_panel_floor(d);
    let lower_capacity = disk_lower_band_capacity(budget.content_height);
    let activity_available =
        !activity_samples.is_empty() && budget.chart_inventory == PerformanceChartInventory::Full;
    let usage_available = d.partitions.len() <= MAX_VISIBLE_MOUNTED_PARTITIONS;
    let partition_fits = lower_band_fits(lower_capacity, partition_floor);
    let activity_fits = lower_band_fits(
        lower_capacity,
        partition_floor + DISK_LOWER_GAP + DISK_ACTIVITY_SECTION_FLOOR,
    );
    let usage_after_activity_fits = lower_band_fits(
        lower_capacity,
        partition_floor
            + if activity_available && activity_fits {
                DISK_LOWER_GAP + DISK_ACTIVITY_SECTION_FLOOR
            } else {
                0.0
            }
            + DISK_LOWER_GAP
            + DISK_USAGE_PANEL_FLOOR,
    );
    let usage_without_activity_fits = lower_band_fits(
        lower_capacity,
        partition_floor + DISK_LOWER_GAP + DISK_USAGE_PANEL_FLOOR,
    );
    let show_activity = partition_fits && activity_available && activity_fits;
    let show_usage = partition_fits
        && usage_available
        && if show_activity {
            usage_after_activity_fits
        } else {
            usage_without_activity_fits
        };
    let activity_graph = show_activity.then(|| {
        render_chart(
            theme,
            ChartSpec::secondary(
                "disk-activity-graph",
                (ElementId::from("disk-activity-graph"), d.device_id.clone()),
                i18n::t("disk.active_time").to_string(),
                Rc::clone(&activity_samples),
                theme.disk,
                GraphUnit::Percent,
            ),
            performance.graph,
            budget.vertical,
            hover_slot,
        )
        .into_any_element()
    });
    let stats = disk_stats(d, units, temperature_samples.as_ref());
    let has_smart = has_smart_fields(d); // ── SMART health button (opens a dedicated attributes dialog) ──
    let smart_footer: Option<AnyElement> = if smart_section_visible(d) {
        if has_smart {
            let i_val = i;
            Some(
                div()
                    .id("disk-smart-btn")
                    .focusable()
                    .tab_stop(true)
                    .focus(elements::focus_ring(theme))
                    .cursor_pointer()
                    .on_click(cx.listener(move |v, _ev, _win, cx| {
                        v.show_disk_smart(i_val);
                        cx.notify();
                    }))
                    .child(
                        div()
                            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_12))
                            .text_color(taskmanager_ui::theme_binding::hsla(theme.accent))
                            .child(i18n::t("disk.smart_health")),
                    )
                    .into_any_element(),
            )
        } else {
            status_footer(theme, effective_smart_status(d))
        }
    } else {
        None
    };
    let usage_panel = show_usage
        .then(|| directory_usage::directory_usage_panel(theme, d, directory_usage, units, cx));
    perf_page(PerfPageProps {
        theme,
        title: if d.model.is_empty() {
            d.name.trim_start_matches("/dev/").to_string()
        } else {
            d.model.clone()
        },
        subtitle: format!(
            "{} · {} · {}",
            d.name.trim_start_matches("/dev/"),
            d.disk_type,
            d.fs_type
        ),
        // The throughput chart never answers "how full is this disk"; the
        // partition panel can degrade away. This one line carries the
        // capacity fact through EVERY vertical rung.
        vital_line: Some(disk_stats::vital_line(d, units)),
        header_extra: None,
        headline: HeadlineSurface::Charts(vec![
            ChartSpec::dual_headline(
                // finite_series_peak already floors the shared dynamic scale at
                // 1.0, so an all-gap window keeps a neutral axis.
                "main-graph",
                (ElementId::from("tm-perf-main-graph"), d.device_id.clone()),
                DualLanes {
                    aggregate: samples,
                    primary: read_samples,
                    primary_label: i18n::t("disk.read"),
                    secondary: write_samples,
                    secondary_label: i18n::t("disk.write"),
                },
                theme.disk,
                GraphUnit::DriveRate(units),
            )
            .with_max(observed_max),
        ]),
        below: partition_fits.then(|| {
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap(taskmanager_ui::theme_binding::definite_length(
                    tokens::SPACE_8,
                ))
                .children(activity_graph)
                .child(partition_panel(theme, &d.partitions, units))
                .children(usage_panel)
                .into_any_element()
        }),
        stats: stats_panel(theme, stats, budget.details, budget.content_height),
        stats_footer: smart_footer,
        hover_slot,
        graph_settings: performance.graph,
        budget,
    })
}

/// Stateless renderer inputs for one network detail page.
pub(crate) struct NetworkViewProps<'a> {
    pub(crate) theme: &'a Theme,
    pub(crate) snap: &'a SystemSnapshot,
    pub(crate) telemetry: &'a TelemetryStore,
    pub(crate) index: usize,
    pub(crate) performance: PerformanceSettings,
    pub(crate) hover_slot: &'a Rc<RefCell<Option<GraphHover>>>,
    pub(crate) budget: PerformancePageBudget,
}

pub(crate) fn render_network(props: NetworkViewProps<'_>) -> Div {
    let NetworkViewProps {
        theme,
        snap,
        telemetry,
        index: i,
        performance,
        hover_slot,
        budget,
    } = props;
    let units = performance.units;
    let Some(n) = snap.networks.get(i) else {
        return div();
    };
    // Split-direction throughput windows (receive = family token, send = the
    // same token lifted toward white); the summed per-NIC lane keeps the
    // aggregate summary and first-frame state honest. The shared max is the
    // link-speed ceiling when dynamic scaling is off, otherwise the greater
    // finite peak of the two directions, so rx and tx stay directly
    // comparable.
    let rx_samples =
        network_rx_rate_samples(&telemetry.system_history, &n.device_id, n.device_generation);
    let tx_samples =
        network_tx_rate_samples(&telemetry.system_history, &n.device_id, n.device_generation);
    let samples =
        network_rate_samples(&telemetry.system_history, &n.device_id, n.device_generation);
    let observed_max = finite_series_peak(&rx_samples).max(finite_series_peak(&tx_samples));
    let max = if performance.graph.network_dynamic_scaling {
        observed_max
    } else {
        network_link_speed_graph_max(n).unwrap_or(observed_max)
    };
    // Title surfaces the SSID for wireless adapters (MC shows the network name
    // as the card heading); falls back to the interface name when not associated.
    let is_wireless = n.adapter_type() == NetworkAdapterType::WiFi;
    let title = network_title(n, is_wireless);
    let stats = network_stats(n, is_wireless, units);
    perf_page(PerfPageProps {
        theme,
        title,
        subtitle: n.ipv4_addr.as_deref().unwrap_or_default().to_owned(),
        vital_line: Some(network_stats::vital_line(n)),
        header_extra: None,
        headline: HeadlineSurface::Charts(vec![
            ChartSpec::dual_headline(
                "main-graph",
                (ElementId::from("tm-perf-main-graph"), n.device_id.clone()),
                DualLanes {
                    aggregate: samples,
                    primary: rx_samples,
                    primary_label: i18n::t("net.receive"),
                    secondary: tx_samples,
                    secondary_label: i18n::t("net.send"),
                },
                theme.network,
                GraphUnit::NetworkRate(units),
            )
            .with_max(max),
        ]),
        below: None,
        stats: stats_panel(theme, stats, budget.details, budget.content_height),
        stats_footer: status_footer(theme, n.device_state.status),
        hover_slot,
        graph_settings: performance.graph,
        budget,
    })
}

// ---- shared helpers ----

// ---- shared helpers ----

/// Greatest finite sample of a window, floored — the shared dynamic max of a
/// throughput graph. `f32::max` ignores the NaN gaps, so an all-gap (or
/// empty) window floors to the given neutral scale instead of a degenerate
/// zero.
pub(super) fn finite_series_peak_floored(floor: f32, samples: &[f32]) -> f32 {
    samples
        .iter()
        .copied()
        .filter(|value| value.is_finite())
        .fold(floor, f32::max)
}

/// `finite_series_peak_floored` at the throughput floor (1.0 MB/s) — the
/// shared dynamic max of a two-series disk/network graph.
pub(super) fn finite_series_peak(samples: &[f32]) -> f32 {
    finite_series_peak_floored(1.0, samples)
}

pub(super) fn graph_summary_row(
    theme: &Theme,
    samples: &[f32],
    format_value: &dyn Fn(f32) -> String,
) -> Option<Div> {
    let summary = finite_graph_summary(samples)?;
    Some(
        div()
            .flex()
            .flex_row()
            .gap(taskmanager_ui::theme_binding::definite_length(
                tokens::SPACE_16,
            ))
            .text_size(taskmanager_ui::theme_binding::font_size(tokens::FONT_11))
            .text_color(taskmanager_ui::theme_binding::hsla(theme.fg_dim))
            .child(format!(
                "{} {}",
                i18n::t("common.latest"),
                format_value(summary.latest)
            ))
            .child(format!(
                "{} {}",
                i18n::t("common.avg"),
                format_value(summary.average)
            ))
            .child(format!(
                "{} {}",
                i18n::t("common.peak"),
                format_value(summary.maximum)
            )),
    )
}

fn finite_graph_summary(samples: &[f32]) -> Option<taskmanager_shell::presentation::GraphSummary> {
    taskmanager_shell::presentation::graph_summary(samples)
}

pub(super) fn rate_str(units: UnitPreferences, bytes_per_sec: u64) -> String {
    units.format_quantity(bytes_per_sec, QuantityFamily::Network, true)
}

pub(super) fn drive_rate_str(units: UnitPreferences, bytes_per_sec: u64) -> String {
    units.format_quantity(bytes_per_sec, QuantityFamily::Drive, true)
}

#[cfg(test)]
#[path = "../../tests/gui/gpui_gpui_app_perf_views_readout_tests.rs"]
mod readout_tests;
