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
//! zero-height area and an honest empty panel for `None` / no fan readings, so
//! a desktop host (or a tick before the first sensor batch lands) never reads
//! as a fabricated idle fan. RPM is the headline reading per fan; PWM and the
//! temperatures of the same physical device are appended only when the
//! provider actually reports them.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};

use taskmanager_application::i18n::t;
use taskmanager_core::core::sensors::{
    SensorCenterSnapshot, SensorMagnitude, SensorQuantity, SensorReading,
};
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::missing_value;

use crate::TuiApp;
use crate::TuiTheme;

/// Render the per-fan detail section into `area`. A zero-height area (the
/// small-terminal case where no panel was allocated) renders nothing. A `None`
/// snapshot or a snapshot without fan readings renders an honest empty panel —
/// never a fabricated idle fan. Each fan channel also carries its OWN one-line
/// RPM sparkline (that channel's window from the shared `LiveGraphHistory`),
/// mirroring the per-device trend on the Disk/Network/GPU pages; RPM
/// auto-scales to its finite peak (NOT 0..100).
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
    if !sensors
        .readings
        .iter()
        .any(|reading| reading.quantity() == &SensorQuantity::FanSpeed)
    {
        super::render_empty_panel(frame, theme, area, t("common.fan"), t("fan.empty"));
        return;
    }
    let lines = fan_lines(sensors, &app, theme, app.prefs.graph_points);
    frame.render_widget(
        Paragraph::new(lines)
            .block(super::panel(t("common.fan"), theme))
            .wrap(Wrap { trim: true }),
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
