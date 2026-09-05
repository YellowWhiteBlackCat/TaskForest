//! Threshold-suggestions overlay rendered on top of the live TUI frame.
//!
//! Surfaces the shared [`AlertEngine::suggest_threshold`] heuristic that the
//! `--suggest-thresholds` CLI already exposes, but driven from the TUI's own
//! rolling window (see [`taskmanager_telemetry_store::live_graph::LiveGraphHistory`]) so the proposal
//! becomes principled once enough telemetry accrues.
//!
//! Honesty contract (mirrored verbatim from `src/cli.rs`): an `Insufficient`
//! verdict is rendered as its typed marker — `too_few_samples (N/20)` or
//! `unsupported_metric` — and NEVER as a fabricated number. A `Suggested`
//! verdict shows the threshold, its clear-band hysteresis, the derivation
//! basis, the confidence band, and the sample count, so a user can see "why
//! this number?" instead of a bare value. The window starts empty, so early on
//! every numeric metric honestly renders as insufficient.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use taskmanager_application::ManagedAlertRule;
use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::{
    AlertEvent, AlertEventKind, AlertMetric, AlertSeverity, InsufficientReason,
    SUGGESTION_MIN_SAMPLES, SuggestedThreshold, SuggestionBasis, SuggestionConfidence,
};
use taskmanager_ui_contract::IconId;

use crate::TuiTheme;

/// Render the threshold-suggestions overlay centred over `area`. Does nothing
/// if the terminal is too small for a readable box. The overlay is driven by
/// [`crate::TuiApp::history`]; it never reads the point-in-time snapshot directly, so
/// a stale latest snapshot cannot leak a fabricated threshold into the view.
pub(super) fn render_suggestions_overlay_at(
    frame: &mut Frame<'_>,
    app: &crate::TuiApp,
    theme: TuiTheme,
    popup: Rect,
) {
    frame.render_widget(Clear, popup);
    let block = Block::new()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(theme.accent))
        .style(Style::new().bg(theme.overlay_bg))
        .title(format!(
            " {} {} ",
            theme.glyph(IconId::Settings),
            t("alerts.threshold_suggestions")
        ));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(8), Constraint::Length(2)]).areas(inner);

    let mut lines: Vec<Line<'static>> = Vec::with_capacity(AlertMetric::ALL.len() + 2);
    lines.push(Line::from(vec![
        Span::styled(t("alerts.floor_hint"), Style::new().fg(theme.dim)),
        Span::raw(" "),
        Span::styled(
            t("alerts.samples_required").replacen("{}", &SUGGESTION_MIN_SAMPLES.to_string(), 1),
            Style::new().fg(theme.dim).add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(Line::from(""));

    let history = app.alert_suggestions.clone();
    for metric in AlertMetric::ALL {
        lines.push(metric_line(metric, history.suggest(metric), theme));
    }

    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), body);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    " T / Esc ",
                    Style::new().fg(theme.color(Color::Black)).bg(theme.accent),
                ),
                Span::styled(
                    format!("  {}   ", t("chrome.close")),
                    Style::new().fg(theme.dim),
                ),
                Span::styled(t("alerts.live_hint"), Style::new().fg(theme.dim)),
            ]),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

/// Build one overlay row: the metric label (left) and the honest verdict
/// (right). A `Suggested` verdict shows its value + clear band + basis +
/// confidence + sample count; an `Insufficient` verdict shows its typed marker
/// and never a fabricated number. `pub(crate)` so the health page renders the
/// same rows as the suggestions overlay.
pub(crate) fn metric_line(
    row: AlertMetric,
    suggestion: SuggestedThreshold,
    theme: TuiTheme,
) -> Line<'static> {
    let label = Span::styled(
        super::text::pad_cells(metric_label(row), 18),
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
    );
    match suggestion {
        SuggestedThreshold::Suggested {
            threshold,
            hysteresis,
            basis,
            sample_count,
            confidence,
            ..
        } => {
            let value_color = match row {
                AlertMetric::DiskTemperatureC => warn_for_temperature(threshold, theme),
                _ => warn_for_percentage(threshold, theme),
            };
            Line::from(vec![
                label,
                Span::raw(" "),
                Span::styled(
                    format!("{}{}", format_threshold(threshold), metric_unit(row)),
                    Style::new().fg(value_color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  ±{} clear", format_threshold(hysteresis)),
                    Style::new().fg(theme.dim),
                ),
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{} · {} · n={}",
                        basis_label(basis),
                        confidence_label(confidence),
                        sample_count
                    ),
                    Style::new().fg(theme.dim),
                ),
            ])
        }
        SuggestedThreshold::Insufficient {
            sample_count,
            required,
            reason,
        } => {
            let (marker, color) = insufficient_marker(reason, sample_count, required, theme);
            Line::from(vec![
                label,
                Span::raw(" "),
                Span::styled(marker, Style::new().fg(color)),
            ])
        }
    }
}

pub(crate) fn metric_label(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent => t("alerts.metric_cpu"),
        AlertMetric::MemoryUsagePercent => t("alerts.metric_memory"),
        AlertMetric::DiskTemperatureC => t("alerts.metric_disk_temperature"),
        AlertMetric::SmartPercentUsed => t("alerts.metric_smart_used"),
        AlertMetric::SmartCriticalWarning => t("alerts.metric_smart_critical"),
    }
}

/// Unit suffix appended to a suggested threshold value. The binary
/// SMART-warning metric never reaches the suggested branch, so it carries no
/// unit.
pub(crate) fn metric_unit(metric: AlertMetric) -> &'static str {
    match metric {
        AlertMetric::CpuUsagePercent
        | AlertMetric::MemoryUsagePercent
        | AlertMetric::SmartPercentUsed => "%",
        AlertMetric::DiskTemperatureC => "°C",
        AlertMetric::SmartCriticalWarning => "",
    }
}

/// Compact projection of one canonical managed rule for the Health surface,
/// highlighting the active selection cursor. Disabled rules remain present
/// and are labelled instead of being mistaken for deletion.
pub(crate) fn managed_rule_line(
    managed: &ManagedAlertRule,
    active: bool,
    theme: TuiTheme,
) -> Line<'static> {
    let severity = match managed.rule.severity {
        AlertSeverity::Info => theme.accent,
        AlertSeverity::Warning => theme.warn,
        AlertSeverity::Critical => theme.danger,
    };
    let state = if managed.enabled {
        t("common.enabled")
    } else {
        t("common.disabled")
    };
    let state_color = if managed.enabled {
        theme.good
    } else {
        theme.dim
    };
    let prefix = if active { "› " } else { "  " };
    let prefix_style = if active {
        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(theme.dim)
    };
    let (metric_style, state_style) = if active {
        (
            Style::new()
                .fg(severity)
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD),
            Style::new()
                .fg(state_color)
                .bg(theme.highlight_bg)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        (
            Style::new().fg(severity).add_modifier(Modifier::BOLD),
            Style::new().fg(state_color),
        )
    };
    Line::from(vec![
        Span::styled(prefix, prefix_style),
        Span::styled(
            super::text::pad_cells(metric_label(managed.rule.metric), 18),
            metric_style,
        ),
        Span::styled(
            format!(
                " {:.1}{} · {}",
                managed.rule.threshold,
                metric_unit(managed.rule.metric),
                state
            ),
            state_style,
        ),
    ])
}

/// Compact projection of one alert event transition for the Health surface.
pub(crate) fn alert_event_line(event: &AlertEvent, theme: TuiTheme) -> Line<'static> {
    let (kind_str, kind_color) = match event.kind {
        AlertEventKind::Activated => {
            let color = match event.alert.severity {
                AlertSeverity::Critical => theme.danger,
                AlertSeverity::Warning => theme.warn,
                AlertSeverity::Info => theme.accent,
            };
            (t("events.active"), color)
        }
        AlertEventKind::Cleared => (t("events.cleared"), theme.good),
    };
    let unit = metric_unit(event.alert.metric);
    let target = if event.alert.target.is_empty() {
        metric_label(event.alert.metric)
    } else {
        event.alert.target.as_str()
    };
    Line::from(vec![
        Span::styled(
            format!("  {}ms ", event.observed_at_ms),
            Style::new().fg(theme.dim),
        ),
        Span::styled(
            format!("[{kind_str}] "),
            Style::new().fg(kind_color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!("{target} "),
            Style::new().fg(theme.color(Color::White)),
        ),
        Span::styled(
            format!(
                "· {} {:.1}{} (thresh: {:.1}{})",
                metric_label(event.alert.metric),
                event.alert.value,
                unit,
                event.alert.threshold,
                unit,
            ),
            Style::new().fg(theme.dim),
        ),
    ])
}

fn format_threshold(value: f32) -> String {
    format!("{value:.1}")
}

/// Honest marker for an `Insufficient` verdict, mirroring the CLI's
/// `insufficient_reason_str` snake-case spelling so the two surfaces agree.
fn insufficient_marker(
    reason: InsufficientReason,
    sample_count: usize,
    required: usize,
    theme: TuiTheme,
) -> (String, Color) {
    match reason {
        InsufficientReason::TooFewSamples => (
            format!("insufficient · too_few_samples ({sample_count}/{required})"),
            theme.warn,
        ),
        InsufficientReason::UnsupportedMetric => (
            "insufficient · unsupported_metric".to_string(),
            theme.danger,
        ),
    }
}

fn warn_for_percentage(value: f32, theme: TuiTheme) -> Color {
    if value >= 90.0 {
        theme.danger
    } else if value >= 75.0 {
        theme.warn
    } else {
        theme.good
    }
}

fn warn_for_temperature(value: f32, theme: TuiTheme) -> Color {
    if value >= 70.0 {
        theme.danger
    } else if value >= 55.0 {
        theme.warn
    } else {
        theme.good
    }
}

fn basis_label(basis: SuggestionBasis) -> &'static str {
    match basis {
        SuggestionBasis::MeanPlusStddevFloorP95 => "mean+3σ∧p95",
    }
}

fn confidence_label(confidence: SuggestionConfidence) -> &'static str {
    match confidence {
        SuggestionConfidence::Low => "low",
        SuggestionConfidence::High => "high",
    }
}

#[cfg(test)]
#[path = "../../tests/gui/ui/alerts_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/ui/alerts_support.rs"]
pub(crate) mod alerts_support;
