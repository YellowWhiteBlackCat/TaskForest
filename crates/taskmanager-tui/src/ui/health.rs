//! System-health and alert-rules overlay.
//!
//! Two honest sections over the live frame:
//!
//! - **Device summary**: one verdict row per telemetry domain (CPU, memory,
//!   storage, network, GPU, containers) plus the hardware inventory, derived
//!   from the typed snapshot / rollup / hardware facts. Missing telemetry
//!   renders a dim "collecting" marker, never a fabricated healthy verdict.
//! - **Alert rules**: the canonical application-owned managed rules, including
//!   disabled entries. Threshold suggestions remain on the dedicated `T`
//!   overlay and cannot become a second editable rule authority.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_application::{
    SourceLineProjection, SourceStateKind, device_source_line, truncate_text,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{ProviderRuntimeState, SystemSnapshot};
use taskmanager_ui_contract::IconId;

use super::containers::KeyHint;
use super::containers::Modal;
use super::health_data::{
    Verdict, cpu_value, cpu_verdict, gpu_value, gpu_verdict, memory_value, memory_verdict,
    network_value, network_verdict, storage_value, storage_verdict,
};
use crate::TuiApp;

#[cfg(test)]
#[path = "../../tests/headless/ui/health_support.rs"]
pub(crate) mod health_support;
use crate::TuiTheme;
use crate::ui::alerts::managed_rule_line;
use crate::ui::{DeviceHealth, classify_device_state};

/// Render the health overlay centred over `area`.
pub(super) fn render_health_overlay_at(
    frame: &mut Frame<'_>,
    app: &TuiApp,
    theme: TuiTheme,
    popup: Rect,
) {
    let inner =
        Modal::new(theme, IconId::Health, t("health.system_health_alerts")).render(frame, popup);

    let [summary, rules, footer] = Layout::vertical([
        Constraint::Length(12),
        Constraint::Min(6),
        Constraint::Length(3),
    ])
    .areas(inner);

    render_device_summary(frame, app, theme, summary);
    render_alert_rules(frame, app, theme, rules);

    let mut hint = KeyHint::spans(
        theme,
        crate::command_palette::surface_hint_pairs(
            crate::command_palette::TuiSurfaceScope::StatusOverlay,
            crate::command_palette::TuiSurfaceAction::ToggleHealth,
        ),
    );
    hint.push(Span::styled(t("health.t_hint"), Style::new().fg(theme.dim)));
    frame.render_widget(
        Paragraph::new(vec![Line::from(""), Line::from(hint)]).alignment(Alignment::Center),
        footer,
    );
}

fn render_device_summary(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let [heading, rows] = Layout::vertical([Constraint::Length(1), Constraint::Min(8)]).areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t("health.device_status"),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))),
        heading,
    );

    let snapshot = app.projection().snapshot.as_ref();
    let lines = vec![
        domain_line(
            theme,
            t("common.cpu"),
            cpu_value(snapshot),
            cpu_verdict(snapshot),
        ),
        domain_line(
            theme,
            t("common.memory"),
            memory_value(snapshot),
            memory_verdict(snapshot),
        ),
        domain_line(
            theme,
            t("health.domain_storage"),
            storage_value(snapshot),
            storage_verdict(snapshot),
        ),
        domain_line(
            theme,
            t("sidebar.network"),
            network_value(snapshot),
            network_verdict(snapshot),
        ),
        domain_line(
            theme,
            t("common.gpu"),
            gpu_value(snapshot),
            gpu_verdict(snapshot),
        ),
        domain_line(
            theme,
            t("containers.title"),
            containers_value(app),
            containers_verdict(app),
        ),
    ];
    let mut all = lines;
    match app.projection().hardware.as_ref() {
        None => all.push(domain_line(
            theme,
            t("health.domain_hardware"),
            t("health.inventory_not_collected").to_owned(),
            Verdict::Inactive,
        )),
        Some(hardware) => all.push(domain_line(
            theme,
            t("health.domain_hardware"),
            hardware
                .cpu_brand
                .as_deref()
                .map_or_else(|| t("health.inventory_present").to_owned(), str::to_owned),
            Verdict::Good,
        )),
    }
    all.push(provider_line(theme, &snapshot, app));
    frame.render_widget(Paragraph::new(all).wrap(Wrap { trim: true }), rows);
}

/// One typed verdict per domain; the color carries the state and the text
/// carries the honest reason.
impl Verdict {
    fn color(self, theme: TuiTheme) -> Color {
        match self {
            Self::Good => theme.good,
            Self::Warn => theme.warn,
            Self::Danger => theme.danger,
            Self::Inactive => theme.dim,
        }
    }
}

fn domain_line(theme: TuiTheme, label: &str, value: String, verdict: Verdict) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("  {}", super::text::pad_cells(label, 11)),
            Style::new().fg(theme.dim),
        ),
        Span::styled(value, Style::new().fg(theme.color(Color::White))),
        Span::styled(
            format!("  {}", verdict_label(verdict)),
            Style::new()
                .fg(verdict.color(theme))
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn verdict_label(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Good => t("health.verdict_ok"),
        Verdict::Warn => t("health.verdict_degraded"),
        Verdict::Danger => t("health.verdict_failed"),
        Verdict::Inactive => t("health.verdict_collecting"),
    }
}

fn containers_value(app: &TuiApp) -> String {
    match app.projection().containers.as_ref() {
        None => t("health.not_collected").to_owned(),
        Some(rollup) => {
            t("health.containers_value").replacen("{}", &rollup.containers.len().to_string(), 1)
        }
    }
}

fn containers_verdict(app: &TuiApp) -> Verdict {
    match app.projection().containers.as_ref() {
        None => Verdict::Inactive,
        Some(rollup) => match classify_device_state(&rollup.state) {
            DeviceHealth::Healthy => Verdict::Good,
            DeviceHealth::Stale | DeviceHealth::MissingTool => Verdict::Warn,
            DeviceHealth::PermissionDenied => Verdict::Danger,
            DeviceHealth::Unsupported => Verdict::Inactive,
        },
    }
}

/// Cap for one provider id on the diagnostics line, applied through the
/// shared char-boundary truncation rule. Real ids stay far below it; the cap
/// only bounds the wrapped row for pathological origins.
const PROVIDER_ORIGIN_MAX_CHARS: usize = 24;

fn provider_line(
    theme: TuiTheme,
    snapshot: &Option<&SystemSnapshot>,
    _app: &TuiApp,
) -> Line<'static> {
    let providers: &[ProviderRuntimeState] = snapshot
        .and_then(|snapshot| Some(snapshot.provider_states.as_slice()))
        .unwrap_or(&[]);
    if providers.is_empty() {
        return Line::from(vec![
            Span::styled(
                format!(
                    "  {}",
                    super::text::pad_cells(t("health.domain_providers"), 11)
                ),
                Style::new().fg(theme.dim),
            ),
            Span::styled(
                t("health.no_provider_diagnostics"),
                Style::new().fg(theme.dim),
            ),
        ]);
    }
    let mut spans = vec![Span::styled(
        format!(
            "  {}",
            super::text::pad_cells(t("health.domain_providers"), 11)
        ),
        Style::new().fg(theme.dim),
    )];
    for provider in providers {
        // The `DeviceStatus` enum is not exported across the dependency
        // firewall, so re-wrap the provider's typed status into a public
        // `DeviceState`; the application layer's neutral VM owns the
        // status→kind fold — this view only maps kind→tone and token.
        let runtime = taskmanager_core::core::device_state::DeviceState {
            status: provider.status,
            last_success_ms: provider.last_success_ms,
        };
        let line = device_source_line(&provider.provider, &runtime);
        let origin = truncate_text(&line.origin, PROVIDER_ORIGIN_MAX_CHARS);
        spans.push(Span::styled(
            format!("{}:{} ", origin, provider_status_label(&line)),
            Style::new().fg(source_state_color(line.state, theme)),
        ));
    }
    Line::from(spans)
}

/// Kind → tone for the provider diagnostics line. The TUI's Kind→color
/// decision lives here; tone differences between frontends are preserved by
/// design (the GPUI health page uses its own palette).
fn source_state_color(state: SourceStateKind, theme: TuiTheme) -> Color {
    match state {
        SourceStateKind::Ok => theme.good,
        SourceStateKind::Degraded | SourceStateKind::Stale => theme.warn,
        SourceStateKind::Failed => theme.danger,
        SourceStateKind::Unknown => theme.dim,
    }
}

/// Compact provider token for the diagnostics line. Tokens stay English and
/// terminal-scannable by design; the neutral VM owns status→kind, and this
/// table resolves kind (+ typed cause where two tokens share a kind) → token.
fn provider_status_label(line: &SourceLineProjection) -> &'static str {
    match (line.state, line.failure) {
        (SourceStateKind::Ok, _) => "ok",
        (SourceStateKind::Stale, _) => "stale",
        (SourceStateKind::Degraded, Some(FailureKind::MissingDependency)) => "missing-tool",
        (SourceStateKind::Degraded, _) => "degraded",
        (
            SourceStateKind::Failed,
            Some(FailureKind::PermissionDenied | FailureKind::RequiresEscalation),
        ) => "denied",
        (SourceStateKind::Failed, _) => "failed",
        (SourceStateKind::Unknown, _) => "unsupported",
    }
}

fn render_alert_rules(frame: &mut Frame<'_>, app: &TuiApp, theme: TuiTheme, area: Rect) {
    let [heading, rows] = Layout::vertical([Constraint::Length(1), Constraint::Min(4)]).areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            t("alerts.manage"),
            Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
        ))),
        heading,
    );

    let lines: Vec<Line<'static>> = app
        .projection()
        .alert_center
        .managed_rules()
        .iter()
        .map(|managed| managed_rule_line(managed, theme))
        .collect();
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), rows);
}

#[cfg(test)]
#[path = "../../tests/gui/ui/health_tests.rs"]
mod tests;
