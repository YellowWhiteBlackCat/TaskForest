//! About / system-information overlay.
//!
//! Renders the same hardware facts as the System page plus the TUI build
//! version, in a modal centred over the frame. Missing facts render as `—`
//! (never fabricated), matching the System page's honesty contract.

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_shell::presentation::{MISSING_VALUE, duration, missing_value};
use taskmanager_ui_contract::IconId;

use super::about_data;
use super::containers::{KeyHint, Modal};
use crate::TuiTheme;
use crate::ui::kv;

/// Build version surfaced by every frontend's about surface.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Render the about overlay centred over `area`. Reads `app.projection().hardware`
/// and `app.projection().snapshot`; absent telemetry renders as `—`.
pub(super) fn render_about_overlay_at(
    frame: &mut Frame<'_>,
    app: &crate::TuiApp,
    theme: TuiTheme,
    popup: Rect,
) {
    let inner = Modal::new(theme, IconId::Settings, t("about.title")).render(frame, popup);

    let [body, footer] = Layout::vertical([Constraint::Min(8), Constraint::Length(3)]).areas(inner);

    let hardware = app.projection().hardware.as_ref();
    let snapshot = app.projection().snapshot.as_ref();
    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("{} TUI", product::NAME),
                Style::new()
                    .fg(theme.color(Color::White))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("  v{VERSION}"),
                Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(""),
        kv(
            t("system.hostname"),
            hardware
                .and_then(|item| item.hostname.as_deref())
                .unwrap_or(MISSING_VALUE),
            theme,
        ),
        kv(
            t("common.operating_system"),
            hardware
                .and_then(|item| item.os_version.as_deref())
                .unwrap_or(MISSING_VALUE),
            theme,
        ),
        kv(
            t("system.kernel"),
            hardware
                .and_then(|item| item.kernel_version.as_deref())
                .unwrap_or(MISSING_VALUE),
            theme,
        ),
        kv(
            t("common.cpu"),
            hardware
                .and_then(|item| item.cpu_brand.as_deref())
                .unwrap_or(MISSING_VALUE),
            theme,
        ),
        kv(
            t("common.logical_cores"),
            hardware
                .and_then(|item| item.cpu_cores)
                .map_or_else(missing_value, |cores| cores.to_string()),
            theme,
        ),
        kv(
            t("common.memory"),
            about_data::memory_value(snapshot),
            theme,
        ),
        kv(
            t("common.uptime"),
            snapshot.map_or_else(missing_value, |item| duration(item.uptime_secs)),
            theme,
        ),
    ];
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), body);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            KeyHint::line(
                theme,
                crate::command_palette::surface_hint_pairs(
                    crate::command_palette::TuiSurfaceScope::StatusOverlay,
                    crate::command_palette::TuiSurfaceAction::ToggleAbout,
                ),
            ),
        ])
        .alignment(Alignment::Center),
        footer,
    );
}

#[cfg(test)]
#[path = "../../tests/gui/ui/about_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/headless/ui/about_support.rs"]
pub(crate) mod about_support;
