//! Per-battery detail block for the Performance page.
//!
//! Reads the shell's `power_supplies` snapshot (`Option<PowerSupplySnapshot>`,
//! populated from `PlatformEventBatch::power_supply_events`) through the typed
//! `current_*` accessors so an unavailable field renders an honest dash instead
//! of a fabricated zero — mirroring `crates/taskmanager-gpui/src/gpui_app/perf_views/dynamic.rs`
//! (`render_battery`) and `root/system_health.rs`. Read-only consume of
//! `taskmanager_core::core::power::BatteryInfo`; this crate never mutates the shared
//! snapshot shape. The accessor names agree with the GPUI frontend so the two
//! renderers agree on what "unavailable" means for each battery field.
//!
//! Render contract: the Performance resource selector hands this section the
//! full content area of the Battery tab; the section renders nothing for a
//! zero-height area and an honest empty panel for `None` / no batteries, so a
//! desktop host (or a tick before the first power batch lands) never reads as a
//! fabricated idle battery. A `capacity_pct` of `None` always renders "—", never
//! a believable-but-fabricated "0%".

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
use ratatui::text::Span;
use ratatui::widgets::{Paragraph, Wrap};

use taskmanager_application::i18n::t;
use taskmanager_core::core::power::BatteryInfo;
use taskmanager_shell::ShellApp;

use crate::TuiApp;
use crate::TuiTheme;

/// Render the per-battery detail section into `area`. A zero-height area (the
/// small-terminal case where no panel was allocated) renders nothing. A `None`
/// snapshot or an empty battery vector renders an honest empty panel — never a
/// fabricated idle battery. Each battery block also carries its OWN one-line
/// charge-% sparkline (that battery's window from the shared `LiveGraphHistory`),
/// mirroring the per-device trend on the Disk/Network/GPU pages.
pub(super) fn render_battery_section(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    area: Rect,
    power_supplies: Option<&taskmanager_core::core::power::PowerSupplySnapshot>,
) {
    if area.height == 0 {
        return;
    }
    let Some(supplies) = power_supplies else {
        super::render_empty_panel(frame, theme, area, t("common.battery"), t("battery.empty"));
        return;
    };
    if supplies.batteries.is_empty() {
        super::render_empty_panel(frame, theme, area, t("common.battery"), t("battery.empty"));
        return;
    }
    let lines = battery_lines(&supplies.batteries, &app, theme, app.prefs.graph_points);
    frame.render_widget(
        Paragraph::new(lines)
            .block(super::panel(t("common.battery"), theme))
            .wrap(Wrap { trim: true }),
        area,
    );
}

/// Build one honest detail line set per battery. Each scalar resolves through
/// the typed `current_*` accessors (the same ones GPUI reads), so an unprobed
/// field renders "—" rather than a fabricated 0% / 0.0 W / 0.00 V. The capacity
/// line is always emitted (it is the headline reading); the optional
/// cycles/technology/manufacturer line appears only when at least one of those
/// fields is present, so a minimal node renders no stray "· — · —" noise. Each
/// battery also gets its OWN one-line charge-% sparkline right under its header
/// (same stable id the recorder uses); a battery with <2 samples renders the
/// dotted "collecting" placeholder instead of a fabricated flat line.
fn battery_lines(
    batteries: &[BatteryInfo],
    shell: &ShellApp,
    theme: TuiTheme,
    graph_window: usize,
) -> Vec<ratatui::text::Line<'static>> {
    let mut lines = Vec::with_capacity(batteries.iter().map(battery_body_line_count).sum());
    for (index, battery) in batteries.iter().enumerate() {
        let data = super::perf_data::battery_data(battery);
        // Header: the most specific available name. Mirrors dynamic.rs — the
        // model name wins, then the provider display name, then a generic
        // "Battery <index>" fallback so two unnamed cells stay distinguishable.
        let title = if !battery.model_name.is_empty() {
            battery.model_name.clone()
        } else if !battery.display_name.is_empty() {
            battery.display_name.clone()
        } else {
            format!("{} {}", t("common.battery"), index)
        };
        lines.push(ratatui::text::Line::from(title));

        // Per-battery charge-% trend: this battery's own window (keyed by its
        // stable id, the same key the recorder uses), so the trend lines up with
        // its row. A constant series renders a flat mid-ramp line; <2 samples
        // renders the dotted "collecting" placeholder — never fabricated.
        let window = shell.history.battery_capacity_pct_for(&battery.id);
        lines.push(ratatui::text::Line::from(vec![
            Span::raw("  "),
            Span::styled(
                super::sparkline::device_trend_in(theme.terminal.glyphs, &window, graph_window),
                Style::new().fg(theme.accent),
            ),
        ]));
        if let Some(summary) = super::sparkline::device_summary_line_in(
            theme.terminal.glyphs,
            t("battery.capacity"),
            &window,
            super::sparkline::DeviceSummaryUnit::Percent,
        ) {
            lines.push(ratatui::text::Line::from(format!("  {summary}")));
        }

        // Headline reading: charge percent + status. `None` capacity MUST render
        // "—" (never "0%"); an empty status string is honest absence too.
        lines.push(ratatui::text::Line::from(format!(
            "  {} {} · {} {}",
            t("battery.capacity"),
            data.capacity,
            t("battery.status"),
            data.status,
        )));

        // Power flow (charge/discharge rate magnitude) + live cell voltage. The
        // status string carries the direction (Charging/Discharging); each
        // magnitude is independently unavailable and renders an honest dash.
        lines.push(ratatui::text::Line::from(format!(
            "  {} {} · {} {}",
            t("battery.power"),
            data.power,
            t("battery.voltage"),
            data.voltage,
        )));

        // Degradation health and the one status-applicable runtime estimate.
        // Each renders only when its typed fact is current; both absent → no
        // line at all, so a minimal node shows no stray "— · —" noise.
        if data.health.is_some() || data.time_estimate.is_some() {
            let mut segments = Vec::new();
            if let Some(health) = &data.health {
                segments.push(format!("{} {}", t("battery.health"), health));
            }
            if let Some(estimate) = &data.time_estimate {
                segments.push(estimate.clone());
            }
            lines.push(ratatui::text::Line::from(format!(
                "  {}",
                segments.join(" · ")
            )));
        }

        // Power-flow history: this battery's OWN power window (keyed by its id,
        // the same key the recorder uses), rendered next to the power scalar
        // when at least two samples exist — a single sample cannot show a
        // SHAPE, so no line at all is the honest absence, never a fabricated
        // flat trend.
        let power_history = shell.history.battery_power_w_for(&battery.id);
        if power_history.len() >= 2 {
            lines.push(ratatui::text::Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    super::sparkline::device_trend_in(
                        theme.terminal.glyphs,
                        &power_history,
                        graph_window,
                    ),
                    Style::new().fg(theme.accent),
                ),
            ]));
            if let Some(summary) = super::sparkline::device_summary_line_in(
                theme.terminal.glyphs,
                t("battery.power"),
                &power_history,
                super::sparkline::DeviceSummaryUnit::Watts,
            ) {
                lines.push(ratatui::text::Line::from(format!("  {summary}")));
            }
        }

        // Optional descriptor line: cycles / technology / manufacturer. Each
        // segment is included only when the field carries truth, so a cold node
        // renders nothing here instead of a row of dashes.
        if let Some(descriptor) = data.descriptor {
            lines.push(ratatui::text::Line::from(descriptor));
        }
    }
    lines
}

/// The number of body lines one battery contributes: header + trend + summary +
/// capacity/status + power/voltage + the health/estimate line + the
/// power-history trend/summary pair (at most, when two power samples exist),
/// plus one descriptor line when any optional field is present. Kept as a loose
/// upper bound for the line buffer preallocation.
fn battery_body_line_count(_battery: &BatteryInfo) -> usize {
    9
}

#[cfg(test)]
#[path = "../../tests/gui/ui/perf_battery_tests.rs"]
mod tests;
