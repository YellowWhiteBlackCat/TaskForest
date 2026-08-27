//! Iced System-page Dashboard segment (GPUI System dashboard parity).
//!
//! An independent segment module: `render_system_dashboard` composes the
//! summary cards, the history-window selector, and the events center from the
//! existing shell projection and is self-sufficient headless (the System page
//! wiring lives in `ui::system_table`, owned by a parallel workflow). The
//! window vocabulary is the shared `taskmanager_application::HistoryWindow`
//! (1h / 24h / 7d) — the shell exposes no GPUI-style `TimelineSelection`
//! type, so the segment maps the window selection through that typed shell
//! enum only. Honesty contract: an unobserved fact renders the shared dash,
//! never a fabricated zero, and the events center lists the shell's live
//! active-alert mirror — no persisted event history exists in the shell, so
//! none is invented.

use iced::Length;
use iced::widget::{column, row, text};
use taskmanager_application::HistoryWindow;
use taskmanager_application::alerts::AlertSeverity;
use taskmanager_application::i18n::t;

use crate::app::alerts::active_alert_lines;
use crate::app::{FocusTarget, Message};
use crate::focus;
use crate::theme;
use taskmanager_theme::tokens;

use super::components::{IcedElement, titled_card};
use super::missing_value;

// The summary fold lives in the data layer (`super::system_dashboard_model`)
// per ARCH.md §4.0; re-exported here so the segment module and its mounted
// tests read one import surface.
pub(crate) use super::system_dashboard_model::{DashboardSummaryModel, summary_model};

/// The System-page dashboard segment's typed message vocabulary, carried by
/// [`crate::app::Message::SystemDashboard`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemDashboardMessage {
    /// Select the history window the segment summarizes.
    SelectWindow(HistoryWindow),
}

/// The localized label for one history window (the same shared catalog keys
/// the Performance page's replay window pills use).
pub(crate) fn history_window_label(window: HistoryWindow) -> &'static str {
    t(match window {
        HistoryWindow::OneHour => "perf.replay.window.1h",
        HistoryWindow::TwentyFourHours => "perf.replay.window.24h",
        HistoryWindow::SevenDays => "perf.replay.window.7d",
    })
}

/// Render the System-page dashboard segment from the current app projection.
/// `selected_window` is the caller-owned window state (the wiring in
/// `ui::system_table` owns where it lives); the pills publish
/// [`SystemDashboardMessage::SelectWindow`] for that lane.
pub(crate) fn render_system_dashboard(
    app: &crate::IcedApp,
    selected_window: HistoryWindow,
) -> IcedElement<'_> {
    let theme_snapshot = app.theme();
    let model = summary_model(app.shell.projection());

    let mut segment = column![].spacing(f32::from(tokens::SPACE_12));
    segment = segment.push(summary_card(theme_snapshot, &model));
    segment = segment.push(window_card(theme_snapshot, selected_window));
    segment = segment.push(events_card(app, theme_snapshot));
    segment.into()
}

/// The four summary value columns (CPU / memory / processes / active alerts),
/// GPUI `summary_card` parity in one titled card.
fn summary_card<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    model: &DashboardSummaryModel,
) -> IcedElement<'a> {
    let muted = theme::muted_text_color(theme_snapshot);
    let alert_color = if model.active_alerts == 0 {
        muted
    } else {
        theme::color(theme_snapshot.palette().danger)
    };
    let value = |label: &'static str, display: String, color: iced::Color| {
        column![
            text(label)
                .size(f32::from(tokens::FONT_11))
                .color(theme::muted_text_color(theme_snapshot)),
            text(display)
                .size(f32::from(tokens::FONT_20))
                .color(color)
                .width(Length::Fill),
        ]
        .spacing(f32::from(tokens::SPACE_4))
        .width(Length::Fill)
    };
    let cpu = theme::color(theme_snapshot.cpu);
    let memory = theme::color(theme_snapshot.memory);
    let disk = theme::color(theme_snapshot.disk);
    let values = row![
        value(t("common.cpu"), model.cpu.clone(), cpu),
        value(t("common.memory"), model.memory.clone(), memory),
        value(
            t("dashboard.processes"),
            model
                .processes
                .map_or_else(|| missing_value().to_owned(), |count| count.to_string()),
            disk,
        ),
        value(
            t("dashboard.active_alerts"),
            model.active_alerts.to_string(),
            alert_color,
        ),
    ]
    .spacing(f32::from(tokens::SPACE_12))
    .width(Length::Fill);
    titled_card(theme_snapshot, t("dashboard.title"), values)
}

/// The history-window selector: one choice pill per shared window, the
/// selected window wearing the active pill.
fn window_card<'a>(
    theme_snapshot: &'a taskmanager_theme::Theme,
    selected_window: HistoryWindow,
) -> IcedElement<'a> {
    let mut pills = row![].spacing(f32::from(tokens::SPACE_4));
    for window in HistoryWindow::ALL {
        // The focus registry is a parallel workflow's file; the window pills
        // reuse the replay window target — the two selectors are never
        // rendered simultaneously (Performance page vs System segment).
        pills = pills.push(focus::choice_pill(
            theme_snapshot,
            FocusTarget::HistoryReplayWindow(window),
            history_window_label(window).to_owned(),
            window == selected_window,
            Message::SystemDashboard(SystemDashboardMessage::SelectWindow(window)),
        ));
    }
    titled_card(
        theme_snapshot,
        t("dashboard.history"),
        pills.width(Length::Fill),
    )
}

/// The events center segment. The shell has no persisted event history
/// projection, so the card lists the live active-alert mirror (real current
/// facts) and renders the honest empty state otherwise — it never fabricates
/// historical events.
fn events_card<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a taskmanager_theme::Theme,
) -> IcedElement<'a> {
    let muted = theme::muted_text_color(theme_snapshot);
    let lines = active_alert_lines(app);
    let mut list = column![].spacing(f32::from(tokens::SPACE_4));
    if lines.is_empty() {
        list = list.push(
            text(t("common.none"))
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        );
    } else {
        for line in lines {
            let color = match line.severity {
                AlertSeverity::Critical => theme::color(theme_snapshot.palette().danger),
                AlertSeverity::Warning => theme::color(theme_snapshot.palette().warning),
                AlertSeverity::Info => theme::color(theme_snapshot.palette().accent),
            };
            list = list.push(
                text(line.text)
                    .size(f32::from(tokens::FONT_12))
                    .style(move |_theme| iced::widget::text::Style { color: Some(color) }),
            );
        }
    }
    titled_card(theme_snapshot, t("dashboard.active_alerts"), list)
}

#[cfg(test)]
#[path = "../../tests/gui/ui/system_dashboard_tests.rs"]
mod tests;
