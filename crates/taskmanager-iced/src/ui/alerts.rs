//! The Iced Alerts page: rule list, active-alert banner, empty state.
//!
//! Pure projection over shared shell state (ADR-027): the rule rows mirror
//! the frontend-local managed list ([`crate::app::alerts`]), current values
//! come from the typed snapshot accessors (an unobserved metric renders the
//! localized `None`, never a fabricated `0`), and the active-alert banner
//! reads the shell-owned `alert_active` evaluation mirror — the view never
//! evaluates alerts itself. Vocabulary reuses the shared `alerts.*` /
//! `alert.*` catalog keys the TUI suggestions overlay and the GPUI rule
//! manager already consume.

use iced::widget::{column, container, row, scrollable, text};
use iced::{Element, Length};
use taskmanager_application::AlertRuleImportMode;
use taskmanager_application::i18n::t;
use taskmanager_core::core::alerts::AlertRuleConflictPolicy;
use taskmanager_core::core::alerts::AlertSeverity;
use taskmanager_ui_contract::IconId;

use crate::app::alerts::{active_alert_lines, empty_state_text, rule_rows};
mod editor;
use crate::app::{AlertsMessage, FocusTarget, Message};
use crate::focus;
use crate::theme;
use taskmanager_theme::Theme;
use taskmanager_theme::tokens;

fn severity_color(severity: AlertSeverity, theme_snapshot: &Theme) -> iced::Color {
    let palette = theme_snapshot.palette();
    match severity {
        AlertSeverity::Critical => crate::theme_binding::color(palette.danger),
        AlertSeverity::Warning => crate::theme_binding::color(palette.warning),
        AlertSeverity::Info => crate::theme_binding::color(palette.accent),
    }
}

/// The page-tab pill for the nav strip. A focusable selection pill (the
/// tab-strip peer of the shared pages' pills) registered under
/// `FocusTarget::AlertsPageTab`, so the frontend-local route is Tab-reachable
/// exactly like the seven shared tabs.
pub(crate) fn page_tab_pill(
    app: &crate::IcedApp,
) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    focus::choice_pill_with_icon(
        theme_snapshot,
        FocusTarget::AlertsPageTab,
        IconId::Alert,
        t("alerts.manage").to_string(),
        app.alerts_page_open(),
        Message::Alerts(AlertsMessage::OpenPage),
    )
}

/// Render the Alerts page body.
pub(crate) fn render(app: &crate::IcedApp) -> Element<'_, Message, iced::Theme, iced::Renderer> {
    let theme_snapshot = app.theme();
    let export_button = focus::button(
        theme_snapshot,
        FocusTarget::AlertsExport,
        t("common.export"),
        Message::Alerts(AlertsMessage::ExportRules),
        false,
    );
    let import_button = focus::button(
        theme_snapshot,
        FocusTarget::AlertsImport,
        "Import (merge)",
        Message::Alerts(AlertsMessage::ImportRulesFromClipboard {
            mode: AlertRuleImportMode::Merge(AlertRuleConflictPolicy::ReplaceExisting),
        }),
        false,
    );
    let replace_button = focus::button(
        theme_snapshot,
        FocusTarget::AlertsImportReplace,
        "Import (replace)",
        Message::Alerts(AlertsMessage::ImportRulesFromClipboard {
            mode: AlertRuleImportMode::Replace,
        }),
        false,
    );

    let add = focus::button(
        theme_snapshot,
        FocusTarget::AlertsAdd,
        t("alerts.add_rule"),
        Message::Alerts(AlertsMessage::AddDefaultRule),
        false,
    );
    let heading = column![
        text(t("alerts.manage")).size(f32::from(tokens::FONT_14)),
        row![add, export_button, import_button, replace_button]
            .spacing(f32::from(tokens::SPACE_8))
            .wrap()
    ];

    let active_section = active_section(app, theme_snapshot);
    let rules_section = rules_section(app, theme_snapshot);

    let body = column![active_section, rules_section]
        .spacing(f32::from(tokens::SPACE_8))
        .width(Length::Fill);

    column![
        heading,
        scrollable(body).width(Length::Fill).height(Length::Fill)
    ]
    .spacing(f32::from(tokens::SPACE_8))
    .height(Length::Fill)
    .into()
}

fn active_section<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let lines = active_alert_lines(app);
    let mut section = column![
        text(t("dashboard.active_alerts"))
            .size(f32::from(tokens::FONT_12))
            .color(muted)
    ]
    .spacing(f32::from(tokens::SPACE_4));

    if lines.is_empty() {
        section = section.push(
            text(t("common.none"))
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        );
    } else {
        for line in lines {
            let color = severity_color(line.severity, theme_snapshot);
            section = section.push(
                text(line.text)
                    .size(f32::from(tokens::FONT_12))
                    .style(move |_theme| iced::widget::text::Style { color: Some(color) }),
            );
        }
    }
    container(section)
        .padding(f32::from(tokens::SPACE_8))
        .width(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into()
}

fn rules_section<'a>(
    app: &crate::IcedApp,
    theme_snapshot: &'a Theme,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let muted = theme::muted_text_color(theme_snapshot);
    let rows = app.alerts_rules();

    if rows.is_empty() {
        return container(
            text(empty_state_text())
                .size(f32::from(tokens::FONT_12))
                .color(muted),
        )
        .padding(f32::from(tokens::SPACE_16))
        .width(Length::Fill)
        .style(move |_| theme::panel_style(theme_snapshot))
        .into();
    }

    let models = rule_rows(app);
    let list = column(
        rows.iter()
            .zip(models)
            .enumerate()
            .map(|(index, (managed, model))| {
                column![
                    text(format!(
                        "{} · {} · {} · {}",
                        model.metric_label,
                        model.severity_label,
                        model.threshold_text,
                        model.current_text
                    )),
                    editor::card(theme_snapshot, index, managed)
                ]
                .into()
            }),
    )
    .spacing(f32::from(tokens::SPACE_8))
    .width(Length::Fill);
    list.into()
}

#[cfg(test)]
#[path = "../../tests/gui/ui/alerts_tests.rs"]
mod tests;
