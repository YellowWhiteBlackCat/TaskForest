//! Per-fan detail section for the Performance page.
//!
//! Reads the shell's `sensors` snapshot (`Option<SensorCenterSnapshot>`,
//! populated from `PlatformEventBatch::sensor_events`) through the typed
//! current-value accessors so an unavailable channel renders an honest dash
//! instead of a fabricated idle RPM — mirroring
//! `src/gpui_app/perf_views/dynamic.rs` (`render_fan`). Each fan channel also
//! carries its OWN per-device RPM mini-graph (auto-scaled to that fan's finite
//! peak) read from `LiveGraphHistory::fan_rpm_for`, the same per-device trend
//! treatment the GPU/disk/network/battery panels give each device row. Read-only
//! consume of `taskmanager_application::SensorReading`; this crate never mutates
//! the shared snapshot shape.

use std::rc::Rc;

use iced::Element;
use iced::widget::{column, container};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    SensorCenterSnapshot, SensorMagnitude, SensorQuantity, SensorReading,
};
use taskmanager_shell::presentation::missing_value;

use super::device_chart;
use super::tables;
use crate::app::Message;
use crate::theme;

/// The Performance-page per-fan panel: one block per fan channel in the shared
/// sensor snapshot, each block topped by a per-fan RPM mini-graph (auto-scaled
/// to that fan's finite peak) plotted from that fan's OWN per-device window. No
/// snapshot → the collecting state; a snapshot without a fan channel → the
/// honest empty line (no fan detected); otherwise each fan's title, its RPM
/// trend, then its honest scalar rows — the same shape as the GPU/disk/network/
/// battery panels. Never a fabricated idle RPM.
pub(super) fn fan_section(
    app: &crate::IcedApp,
    index: usize,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let sensors = app.shell.projection().sensors.as_ref();
    let theme_snapshot = app.theme();
    let color = theme::color(theme_snapshot.fan);
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
                app.compact_layout(),
                true,
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
    container(column(rows).spacing(8))
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

/// One fan's display identity: the channel label when known, else the neutral
/// localized "Fan" label — never an empty heading.
#[must_use]
fn fan_title(fan: &SensorReading) -> String {
    if fan.label().trim().is_empty() {
        t("common.fan").to_string()
    } else {
        format!("{}: {}", t("common.fan"), fan.label().trim())
    }
}

/// Project one fan's honest scalar readouts as label/value rows, mirroring
/// `gpui_app::perf_views::dynamic::render_fan`. RPM is the headline reading:
/// an unknown RPM renders an honest dash, NEVER a fabricated idle 0. PWM and
/// the temperatures of the same physical device are appended only when the
/// provider actually reports them.
#[must_use]
pub(super) fn fan_summary_lines(
    sensors: &SensorCenterSnapshot,
    fan: &SensorReading,
) -> Vec<(String, String)> {
    let mut rows = vec![(
        t("fan.rpm").to_string(),
        fan.current_number()
            .map_or_else(missing_value, |value| format!("{value:.0} RPM")),
    )];
    if let Some(pwm) = fan_pwm_percent(sensors, fan) {
        rows.push((t("fan.pwm").to_string(), format!("{pwm:.0}%")));
    }
    for temperature in sensors.readings.iter().filter(|reading| {
        reading.device_id() == fan.device_id() && reading.quantity() == &SensorQuantity::Temperature
    }) {
        if let Some(value) = temperature.current_number() {
            let name = if temperature.label().trim().is_empty() {
                t("common.temperature").to_string()
            } else {
                format!("{} {}", t("common.temperature"), temperature.label().trim())
            };
            rows.push((name, format!("{value:.1} °C")));
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
/// (the device's OWN window, auto-scaled to its finite peak), then its scalar
/// rows — mirroring the battery/disk/gpu/network block pattern.
fn fan_block<'a>(
    app: &crate::IcedApp,
    sensors: &SensorCenterSnapshot,
    fan: &SensorReading,
    color: iced::Color,
    theme_snapshot: &'a taskmanager_theme::Theme,
    compact: bool,
    smooth: bool,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let samples = app.cached_fan_series(fan.id());
    let mut graphs = vec![device_chart::device_mini_graph_fill(
        samples,
        device_chart::DeviceMetricScale::Rpm,
        color,
        t("fan.rpm").to_string(),
        theme_snapshot,
        compact,
        device_chart::GraphPrefs {
            smooth,
            max_override: None,
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
    if !temperature_samples.is_empty() {
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
                hover: false,
            },
        ));
    }
    super::perf_layout::main_with_stats(
        theme_snapshot,
        fan_title(fan),
        t("fan.speed_graph").to_string(),
        graphs,
        fan_summary_lines(sensors, fan),
        compact,
        super::perf_layout::DetailExtent::for_scroll_parent(compact),
    )
}
