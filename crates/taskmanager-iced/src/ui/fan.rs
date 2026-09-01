//! Per-fan detail section for the Performance page.
//!
//! Reads the shell's `sensors` snapshot (`Option<SensorCenterSnapshot>`,
//! populated from `PlatformEventBatch::sensor_events`) through the typed
//! current-value accessors so an unavailable channel renders an honest dash
//! instead of a fabricated idle RPM — mirroring
//! `src/gpui_app/perf_views/dynamic.rs` (`render_fan`). Each fan channel also
//! carries its OWN per-device RPM mini-graph (auto-scaled to that fan's finite
//! peak, floored at 1000 RPM — GPUI parity) read from
//! `LiveGraphHistory::fan_rpm_for`. Read-only consume of
//! `taskmanager_core::core::sensors::SensorReading`; this crate never mutates the
//! shared snapshot shape.

use std::rc::Rc;

use iced::Element;
use iced::widget::container;
use taskmanager_application::i18n::t;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorMagnitude, SensorQuantity, SensorReading,
};

use taskmanager_shell::viewmodel::StatRow;

use super::device_chart;
use super::responsive::{
    DeviceNavigationPresentation, PerformanceChartInventory, PerformancePageBudget,
};
use super::tables;
use crate::app::Message;
use crate::theme;
use crate::trend_strip::finite_peak;

/// The RPM graph's dynamic-scale floor (GPUI `finite_series_peak_floored`
/// parity): an idle or all-gap window keeps a neutral 1000-RPM axis instead
/// of a degenerate zero.
const FAN_RPM_SCALE_FLOOR: f32 = 1000.0;

/// The Performance-page per-fan panel: one block per fan channel in the shared
/// sensor snapshot, each block topped by a per-fan RPM mini-graph (auto-scaled
/// to that fan's finite peak, floored at 1000 RPM) plotted from that fan's OWN
/// per-device window. No snapshot → the collecting state; a snapshot without a
/// fan channel → the honest empty line (no fan detected); otherwise each fan's
/// title, its RPM trend, then its honest scalar rows — the same shape as the
/// GPU/disk/network/battery panels. Never a fabricated idle RPM.
pub(super) fn fan_section(
    app: &crate::IcedApp,
    index: usize,
    budget: PerformancePageBudget,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let sensors = app.shell.projection().sensors.as_ref();
    let theme_snapshot = app.theme();
    // GPUI parity: the fan family strokes with the CPU token.
    let color = crate::theme_binding::color(theme_snapshot.cpu);
    let compact = budget.device_navigation == DeviceNavigationPresentation::Strip;
    let rows = match (fan_section_state(sensors), sensors) {
        (tables::ListState::Loading, _) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
        (tables::ListState::Empty, _) => {
            vec![tables::message_panel(theme_snapshot, t("fan.empty"))]
        }
        (tables::ListState::Ready, Some(snapshot)) => match snapshot
            .readings
            .iter()
            .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
            .nth(index)
        {
            Some(fan) => vec![fan_block(
                app,
                snapshot,
                fan,
                color,
                theme_snapshot,
                compact,
                true,
                budget,
            )],
            None => vec![tables::message_panel(theme_snapshot, t("fan.empty"))],
        },
        (tables::ListState::Ready, None) => {
            vec![tables::message_panel(
                theme_snapshot,
                t("common.collecting_telemetry"),
            )]
        }
    };
    container(iced::widget::column(rows).spacing(8))
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

/// The per-fan panel readiness, mirroring the battery/GPU/disk/network
/// panels' Loading/Empty/Ready states. `None` (no sensor snapshot observed
/// yet) is Loading; `Some` without a fan channel is Empty (an honest "no fan",
/// not a hidden zero); otherwise Ready.
#[must_use]
pub(super) fn fan_section_state(sensors: Option<&SensorCenterSnapshot>) -> tables::ListState {
    match sensors {
        None => tables::ListState::Loading,
        Some(snapshot)
            if !snapshot
                .readings
                .iter()
                .any(|reading| reading.quantity() == &SensorQuantity::FanSpeed) =>
        {
            tables::ListState::Empty
        }
        Some(_) => tables::ListState::Ready,
    }
}

/// One fan's display identity (GPUI parity): `"Fan — {label}"` when the
/// channel label is known, else the neutral localized "Fan" label — never an
/// empty heading.
#[must_use]
fn fan_title(fan: &SensorReading) -> String {
    if fan.label().trim().is_empty() {
        t("common.fan").to_string()
    } else {
        format!("{} \u{2014} {}", t("common.fan"), fan.label().trim())
    }
}

/// Project one fan's honest scalar readouts as pre-folded shell [`StatRow`]s,
/// mirroring GPUI `perf_views::dynamic_stats::fan_stats`. RPM is the headline
/// reading: an unknown RPM renders an honest dash, NEVER a fabricated idle 0.
/// PWM and the temperatures of the same physical device are appended only
/// when the provider actually reports them.
#[must_use]
pub(super) fn fan_summary_lines(
    sensors: &SensorCenterSnapshot,
    fan: &SensorReading,
) -> Vec<StatRow> {
    let mut rows = vec![StatRow::text(
        t("fan.rpm"),
        fan.current_number().map(|value| format!("{value:.0} RPM")),
    )];
    if let Some(pwm) = fan_pwm_percent(sensors, fan) {
        rows.push(StatRow::text(t("fan.pwm"), Some(format!("{pwm:.0}%"))));
    }
    for temperature in sensors.readings.iter().filter(|reading| {
        reading.device_id() == fan.device_id() && reading.quantity() == &SensorQuantity::Temperature
    }) {
        if let Some(value) = temperature.current_number() {
            rows.push(StatRow::text(
                format!("{} {}", t("common.temperature"), temperature.label()),
                Some(format!("{value:.1} °C")),
            ));
        }
    }
    rows
}

/// The PWM duty-cycle percent for the physical device owning `fan`, resolved
/// through the typed magnitude observation. `None` when no duty-cycle channel
/// is currently readable — never a fabricated 0%.
fn fan_pwm_percent(sensors: &SensorCenterSnapshot, fan: &SensorReading) -> Option<f32> {
    sensors
        .readings
        .iter()
        .filter(|reading| reading.device_id() == fan.device_id())
        .find_map(
            |reading| match reading.measurement_observation().current_value()? {
                SensorMagnitude::DutyCycle { value, maximum } if *maximum > 0 => {
                    Some(*value as f32 * 100.0 / *maximum as f32)
                }
                _ => None,
            },
        )
}

/// One fan's rendered block: the device title line, a per-fan RPM mini-graph
/// (the device's OWN window, auto-scaled to its floored finite peak), the
/// same-device temperature secondary chart while the Full chart inventory
/// keeps secondary charts (GPUI parity — hover-interactive like every shared
/// chart), then its scalar rows and the status footer under the stats rail.
#[allow(clippy::too_many_arguments)]
fn fan_block<'a>(
    app: &'a crate::IcedApp,
    sensors: &SensorCenterSnapshot,
    fan: &SensorReading,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
    smooth: bool,
    budget: PerformancePageBudget,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let samples = app.cached_fan_series(fan.id());
    // GPUI parity: the RPM dynamic scale floors at 1000 so an idle window
    // keeps a neutral axis.
    let rpm_max = finite_peak(&samples).max(FAN_RPM_SCALE_FLOOR);
    let mut graphs = vec![device_chart::device_mini_graph_fill(
        samples,
        device_chart::DeviceMetricScale::Rpm,
        color,
        t("fan.rpm").to_string(),
        theme_snapshot,
        compact,
        device_chart::GraphPrefs {
            smooth,
            max_override: Some(rpm_max),
            // The RPM main graph is hover-interactive; the temperature graph
            // below keeps hover off.
            hover: true,
        },
    )];
    let temperature_samples = sensors
        .readings
        .iter()
        .find(|reading| {
            reading.device_id() == fan.device_id()
                && reading.quantity() == &SensorQuantity::Temperature
        })
        .map_or_else(
            || Rc::from([]),
            |reading| app.cached_fan_temperature_series(reading.id()),
        );
    if !temperature_samples.is_empty() && budget.chart_inventory == PerformanceChartInventory::Full
    {
        graphs.push(device_chart::device_mini_graph_with_height(
            temperature_samples,
            device_chart::DeviceMetricScale::Celsius,
            color,
            t("fan.temperature_graph").to_string(),
            theme_snapshot,
            device_chart::SECONDARY_DEVICE_CHART_HEIGHT,
            device_chart::GraphPrefs {
                smooth,
                max_override: None,
                // GPUI parity: every shared-chart surface is hover-interactive.
                hover: true,
            },
        ));
    }
    super::perf_layout::main_with_stats(
        theme_snapshot,
        fan_title(fan),
        t("fan.speed_graph").to_string(),
        None,
        graphs,
        fan_summary_lines(sensors, fan),
        // GPUI parity: the fan page pins the device-status action hint under
        // the statistics rail.
        super::device_status_footer(theme_snapshot, fan.state().status),
        budget,
        super::perf_layout::DetailExtent::for_scroll_parent(budget.device_navigation),
    )
}
