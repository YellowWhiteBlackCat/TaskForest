//! Per-fan detail block for the Performance page.
//!
//! Reads the shell's `sensors` snapshot (`Option<SensorCenterSnapshot>`,
//! populated from `PlatformEventBatch::sensor_events`) through the typed
//! current-value accessors so an unavailable channel renders an honest dash
//! instead of a fabricated idle RPM — mirroring
//! `crates/taskmanager-gpui/src/gpui_app/perf_views/dynamic.rs` (`render_fan`). Read-only consume of
//! `taskmanager_core::core::sensors::SensorReading`; this crate never mutates the
//! shared snapshot shape.
//!
//! Render contract: the Performance resource selector hands this section the
//! full content area of the Fan tab; the section renders nothing for a
//! zero-height area and an honest empty panel for `None` / no fan and no
//! temperature readings, so a desktop host (or a tick before the first sensor
//! batch lands) never reads as a fabricated idle fan. RPM is the headline
//! reading per fan; PWM and the temperatures of the same physical device are
//! appended only when the provider actually reports them. Below the fan blocks
//! the section carries the SYSTEM thermal-zone group: every `Temperature`
//! reading of the shared sensor center (not only the selected fan's device),
//! one row per reading named by the reading's own source label, with the shared
//! `°C` spelling for an observed value and the shared dash for an unread/failed
//! zone — the same traversal GPUI's health page (`sensor_rows`, Temperature)
//! and iced's `thermal_zone_rows` perform over the same facts.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};

use taskmanager_application::i18n::t;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorMagnitude, SensorQuantity, SensorReading,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::{missing_value, temperature_c_precise};

use crate::TuiApp;
use crate::TuiTheme;

/// Render the per-fan detail section into `area`. A zero-height area (the
/// small-terminal case where no panel was allocated) renders nothing. A `None`
/// snapshot or a snapshot without fan AND temperature readings renders an
/// honest empty panel — never a fabricated idle fan. Each fan channel also
/// carries its OWN one-line RPM sparkline (that channel's window from the
/// shared `LiveGraphHistory`), mirroring the per-device trend on the
/// Disk/Network/GPU pages; RPM auto-scales to its finite peak (NOT 0..100).
/// The system thermal-zone group is admitted only as a whole group: it renders
/// below the fan part when every zone row fits the panel's inner height, and is
/// omitted otherwise (never a half group); a fanless host still states the fan
/// absence and paints the zone group, so the thermal surface never depends on a
/// fan channel existing.
pub(super) fn render_fan_section(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    sensors: Option<&SensorCenterSnapshot>,
) {
    if area.height == 0 {
        return;
    }
    let Some(sensors) = sensors else {
        super::render_empty_panel(frame, theme, area, t("common.fan"), t("fan.empty"));
        return;
    };
    let has_fans = sensors
        .readings
        .iter()
        .any(|reading| reading.quantity() == &SensorQuantity::FanSpeed);
    let has_temperatures = sensors
        .readings
        .iter()
        .any(|reading| reading.quantity() == &SensorQuantity::Temperature);
    if !has_fans && !has_temperatures {
        super::render_empty_panel(frame, theme, area, t("common.fan"), t("fan.empty"));
        return;
    }
    let block = super::panel(t("common.fan"), theme);
    let inner_height = usize::from(block.inner(area).height);
    let mut lines = if has_fans {
        fan_lines(sensors, app, theme, app.prefs.graph_points)
    } else {
        // No fan channel exists (a fanless host, or a tick before the fan
        // provider lands): the fan group states that absence honestly and the
        // system thermal-zone group below stays visible.
        vec![ratatui::text::Line::from(Span::styled(
            format!("  {}", t("fan.empty")),
            Style::new().fg(theme.dim),
        ))]
    };
    let zones = thermal_zone_lines(sensors, theme);
    if !zones.is_empty() && lines.len() + zones.len() <= inner_height {
        lines.extend(zones);
    }
    frame.render_widget(
        Paragraph::new(lines).block(block).wrap(Wrap { trim: true }),
        area,
    );
}

/// Build one honest detail line set per fan channel. Each scalar resolves
/// through the typed current-value accessors (the same ones GPUI reads), so an
/// unprobed channel renders "—" rather than a fabricated 0 RPM. Each channel
/// also gets its OWN one-line RPM sparkline right under its header (same stable
/// label→device_id key the recorder uses); a channel with <2 samples renders
/// the dotted "collecting" placeholder instead of a fabricated flat line.
fn fan_lines(
    sensors: &SensorCenterSnapshot,
    shell: &ShellApp,
    theme: TuiTheme,
    graph_window: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let mut lines = Vec::new();
    for (index, fan) in sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
        .enumerate()
    {
        let title = if !fan.label().is_empty() {
            fan.label().to_owned()
        } else {
            format!("{} {}", t("common.fan"), index)
        };
        lines.push(ratatui::text::Line::from(title));

        // Per-fan RPM trend: this channel's own window (keyed by label→device_id,
        // the same key the recorder uses), so the trend lines up with its row.
        // A constant series renders a flat mid-ramp line; <2 samples renders the
        // dotted "collecting" placeholder — never fabricated.
        let window = shell.history.fan_rpm_for(fan.id());
        lines.push(ratatui::text::Line::from(vec![
            Span::raw("  "),
            Span::styled(
                super::sparkline::device_trend_in(theme.terminal.glyphs, &window, graph_window),
                Style::new().fg(theme.accent),
            ),
        ]));
        if let Some(summary) = super::sparkline::device_summary_line_in(
            theme.terminal.glyphs,
            t("fan.rpm"),
            &window,
            super::sparkline::DeviceSummaryUnit::Rpm,
        ) {
            lines.push(ratatui::text::Line::from(format!("  {summary}")));
        }

        let rpm = fan
            .current_number()
            .map_or_else(missing_value, |value| format!("{value:.0} RPM"));
        let pwm = fan_pwm_percent(sensors, fan)
            .map_or_else(missing_value, |percent| format!("{percent:.0}%"));
        lines.push(ratatui::text::Line::from(format!(
            "  {} {} · {} {}",
            t("fan.rpm"),
            rpm,
            t("fan.pwm"),
            pwm,
        )));

        // Temperatures of the same physical device, one per reported channel.
        // These are the DEVICE-level rows (the fan block's own context); the
        // SYSTEM-level traversal of every temperature reading is the separate
        // `thermal_zone_lines` group below, which names foreign-device zones
        // too and keeps unread zones as named dashes.
        for temperature in sensors.readings.iter().filter(|reading| {
            reading.device_id() == fan.device_id()
                && reading.quantity() == &SensorQuantity::Temperature
        }) {
            if let Some(value) = temperature.current_number() {
                let name = if temperature.label().is_empty() {
                    t("common.temperature").to_string()
                } else {
                    format!("{} {}", t("common.temperature"), temperature.label())
                };
                lines.push(ratatui::text::Line::from(format!(
                    "  {name} · {value:.1} °C"
                )));
            }
        }
        // Temperature history for the same physical device: this channel's OWN
        // window (keyed by label→device_id, the same key the recorder uses),
        // rendered after the scalar rows when at least two samples exist — a
        // single sample cannot show a SHAPE, so no line at all is the honest
        // absence, never a fabricated flat trend.
        let temperature_history = sensors
            .readings
            .iter()
            .find(|reading| {
                reading.device_id() == fan.device_id()
                    && reading.quantity() == &SensorQuantity::Temperature
            })
            .map_or_else(Vec::new, |reading| {
                shell.history.fan_temperature_c_for(reading.id())
            });
        if temperature_history.len() >= 2 {
            lines.push(ratatui::text::Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    super::sparkline::device_trend_in(
                        theme.terminal.glyphs,
                        &temperature_history,
                        graph_window,
                    ),
                    Style::new().fg(theme.accent),
                ),
            ]));
            if let Some(summary) = super::sparkline::device_summary_line_in(
                theme.terminal.glyphs,
                t("common.temperature"),
                &temperature_history,
                super::sparkline::DeviceSummaryUnit::Celsius,
            ) {
                lines.push(ratatui::text::Line::from(format!("  {summary}")));
            }
        }
    }
    lines
}

/// The system thermal-zone group lines: the shared `common.temperature`
/// heading followed by exactly one row per `Temperature` reading in the shared
/// sensor center, in projection order. Each row names the reading's own source
/// label and renders the observed value through the shared
/// [`temperature_c_precise`] spelling; an unread/failed zone keeps its named
/// row with the shared dash, so a denied or cold channel never reads as a
/// fabricated `0.0 °C`. A snapshot with no temperature channel folds to an
/// empty list (the caller omits the whole group). This is the TUI twin of
/// GPUI's health-page `sensor_rows(readings, SensorGroup::Temperature, ..)`
/// and iced's `thermal_zone_rows` over the same typed facts.
fn thermal_zone_lines(
    sensors: &SensorCenterSnapshot,
    theme: TuiTheme,
) -> Vec<ratatui::text::Line<'static>> {
    let mut rows: Vec<ratatui::text::Line<'static>> = sensors
        .readings
        .iter()
        .filter(|reading| reading.quantity() == &SensorQuantity::Temperature)
        .map(|reading| {
            let value = reading
                .current_number()
                .map_or_else(missing_value, |value| temperature_c_precise(value as f32));
            ratatui::text::Line::from(format!("  {} · {value}", reading.label()))
        })
        .collect();
    if rows.is_empty() {
        return rows;
    }
    let mut lines = vec![ratatui::text::Line::from(Span::styled(
        t("common.temperature"),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    ))];
    lines.append(&mut rows);
    lines
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

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_fan_tests.rs"]
mod tests;
