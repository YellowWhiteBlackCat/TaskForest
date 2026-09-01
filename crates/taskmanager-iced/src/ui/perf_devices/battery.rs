//! The per-battery Performance-page detail panel (charge-% trend + the
//! honest scalar rows), extracted from [`super`] so the device-page module
//! stays under the source-size budget. The readiness / title / summary /
//! section / block quintet mirrors the GPU / disk / network panels.

use super::*;
use taskmanager_shell::presentation::duration;
use taskmanager_shell::viewmodel::StatRow;

use super::super::responsive::{
    DeviceNavigationPresentation, PerformanceChartInventory, PerformancePageBudget,
};

// --- Battery --------------------------------------------------------------

/// The Performance-page per-battery panel readiness, mirroring the GPU/disk/
/// network panels' Loading/Empty/Ready states. `None` (no power-supply snapshot
/// observed yet) is Loading; `Some` with an empty battery vector is Empty (an
/// honest "no battery" state, not a hidden zero); otherwise Ready. Private like
/// its siblings — [`tables::ListState`] is `pub(in crate::ui)`.
#[must_use]
pub(crate) fn battery_section_state(
    power_supplies: Option<&PowerSupplySnapshot>,
) -> tables::ListState {
    match power_supplies {
        None => tables::ListState::Loading,
        Some(snapshot) if snapshot.batteries.is_empty() => tables::ListState::Empty,
        Some(_) => tables::ListState::Ready,
    }
}

/// One battery's display identity (GPUI `render_battery` parity): the bare
/// model name when known, else the provider display name, else the neutral
/// localized "Battery {index}" label — never a family prefix and never an
/// empty heading.
#[must_use]
pub(crate) fn battery_title(battery: &BatteryInfo, index: usize) -> String {
    let name = (!battery.model_name.trim().is_empty())
        .then(|| battery.model_name.trim().to_string())
        .or_else(|| {
            (!battery.display_name.trim().is_empty())
                .then(|| battery.display_name.trim().to_string())
        });
    match name {
        Some(name) => name,
        // No identity → fall back to the per-battery index ("Battery 0") so two
        // anonymous batteries stay distinguishable (mirrors GPUI render_battery).
        None => format!("{} {index}", t("common.battery")),
    }
}

/// Project one battery's honest scalar readouts as pre-folded shell
/// [`StatRow`]s for the Performance page, mirroring GPUI
/// `perf_views::dynamic_stats::battery_stats`. Charge is the headline: an
/// unknown capacity renders an honest dash, NEVER a fabricated 0% (the same
/// rule as the GPU/disk percent readouts). The status string conveys charge
/// vs discharge direction; the rate row carries the magnitude. Voltage,
/// cycles, technology and manufacturer are shown only when the provider
/// supplied them so a missing node cannot masquerade as a measured zero.
/// Health derives only from the typed µWh pair, and each runtime estimate
/// appears only when the native source reported one under its status gate —
/// an unavailable estimate is an absent row, never "0%"/"00h 00m".
#[must_use]
pub(crate) fn battery_summary_lines(battery: &BatteryInfo) -> Vec<StatRow> {
    let observed = super::projection::BatteryObservation::from(battery);
    let mut rows = vec![
        StatRow::text(
            t("battery.capacity"),
            observed.capacity_pct.map(|value| format!("{value}%")),
        ),
        StatRow::text(t("battery.status"), Some(battery.status.clone())),
    ];
    if let Some(power) = observed.power_w {
        rows.push(StatRow::text(
            t("battery.power"),
            Some(format!("{power:.1} W")),
        ));
    }
    if let Some(voltage_uv) = observed.voltage_uv {
        rows.push(StatRow::text(
            t("battery.voltage"),
            Some(format!("{:.2} V", voltage_uv as f64 / 1_000_000.0)),
        ));
    }
    if let Some(cycles) = observed.cycle_count {
        rows.push(StatRow::text(t("battery.cycles"), Some(cycles.to_string())));
    }
    if let Some(health) = observed.health_pct {
        rows.push(StatRow::text(
            t("battery.health"),
            Some(format!("{health:.1}%")),
        ));
    }
    if let Some(secs) = observed.time_to_full_secs {
        rows.push(StatRow::text(
            t("battery.time_to_full"),
            Some(duration(secs as u64)),
        ));
    }
    if let Some(secs) = observed.time_to_empty_secs {
        rows.push(StatRow::text(
            t("battery.time_to_empty"),
            Some(duration(secs as u64)),
        ));
    }
    if !battery.technology.trim().is_empty() {
        rows.push(StatRow::text(
            t("battery.technology"),
            Some(battery.technology.trim().to_string()),
        ));
    }
    if !battery.manufacturer.trim().is_empty() {
        rows.push(StatRow::text(
            t("battery.manufacturer"),
            Some(battery.manufacturer.trim().to_string()),
        ));
    }
    rows
}

/// The Performance-page per-battery panel: one block per battery in the shared
/// power-supply snapshot, each block topped by a per-battery charge-% mini-graph
/// (fixed 0..100) plotted from that battery's OWN per-device window. No snapshot
/// → the collecting state; a snapshot with no battery → the honest empty line
/// (no battery detected); otherwise each battery's title, its charge-% trend,
/// then its honest scalar rows — the same shape as the GPU/disk/network panels.
pub(crate) fn battery_section(
    app: &crate::IcedApp,
    index: usize,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let power_supplies = app.shell.projection().power_supplies.as_ref();
    let theme_snapshot = app.theme();
    let color = crate::theme_binding::color(theme_snapshot.battery);
    let compact = budget.device_navigation == DeviceNavigationPresentation::Strip;
    let rows = match (battery_section_state(power_supplies), power_supplies) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("battery.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot.batteries.get(index) {
            Some(battery) => vec![battery_block(
                app,
                battery,
                color,
                theme_snapshot,
                compact,
                index,
                true,
                budget,
            )],
            None => vec![tables::message_panel(theme_snapshot, t("battery.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    device_rows_panel(rows, theme_snapshot)
}

/// One battery's rendered block: the device title line, a per-battery charge-%
/// mini-graph (the device's OWN window, fixed 0..100), the power secondary
/// chart while the Full chart inventory keeps secondary charts (GPUI parity),
/// then its scalar rows — mirroring the GPU/disk/network block shape.
#[allow(clippy::too_many_arguments)]
fn battery_block<'a>(
    app: &'a crate::IcedApp,
    battery: &BatteryInfo,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
    index: usize,
    smooth: bool,
    budget: PerformancePageBudget,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let samples = app.cached_battery_series(&battery.id);
    let graphs = vec![device_chart::device_mini_graph_fill(
        samples,
        device_chart::DeviceMetricScale::Percent,
        color,
        t("battery.capacity").to_string(),
        theme_snapshot,
        compact,
        device_chart::GraphPrefs {
            smooth,
            max_override: None,
            // Both the charge main graph and the power graph below are
            // hover-interactive (crosshair + readout pill at any height).
            hover: true,
        },
    )];
    let mut left = graphs;
    let power_samples = app.cached_battery_power_series(&battery.id);
    // The power secondary chart appears only when the ring holds samples AND
    // the Full chart inventory keeps secondary charts (GPUI parity).
    if !power_samples.is_empty() && budget.chart_inventory == PerformanceChartInventory::Full {
        left.push(device_chart::device_mini_graph_with_height(
            power_samples,
            device_chart::DeviceMetricScale::Watts,
            color,
            t("battery.power_graph").to_string(),
            theme_snapshot,
            device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                hover: true,
            },
        ));
    }
    perf_layout::main_with_stats(
        theme_snapshot,
        battery_title(battery, index),
        t("battery.charge_graph").to_string(),
        None,
        left,
        battery_summary_lines(battery),
        // A non-healthy battery pins its accent-tinted action hint under the
        // statistics rail (GPUI `status_footer` parity): Stale /
        // PermissionDenied / MissingTool surfaces the cause instead of
        // reading like a healthy battery.
        super::device_status_footer(theme_snapshot, battery.device_state.status),
        budget,
        perf_layout::DetailExtent::for_scroll_parent(budget.device_navigation),
    )
}
